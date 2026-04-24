use std::{
    cmp::min,
    collections::{HashMap, HashSet},
};

use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_sdk_types::Condition;
use clvm_traits::{ToClvm, clvm_quote};
use clvm_utils::{ToTreeHash, TreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{AssetInfo, ClawbackV2, DriverError, Puzzle, RequestedPayments};

#[derive(Debug, Clone)]
pub enum RevealedP2Puzzle {
    Clawback(ClawbackV2),
    DelegatedConditions(Vec<Condition>),
}

#[derive(Debug, Clone, Copy)]
pub struct RevealedCoinSpend {
    pub coin: Coin,
    pub puzzle: Puzzle,
    pub solution: NodePtr,
}

#[derive(Debug, Default, Clone)]
pub struct Facts {
    actual_expiration_time: Option<u64>,
    required_expiration_time: Option<u64>,
    reserved_fees: u128,
    coin_spends: HashMap<Bytes32, RevealedCoinSpend>,
    requested_payments: RequestedPayments,
    asset_info: AssetInfo,
    p2_puzzles: HashMap<TreeHash, RevealedP2Puzzle>,
    asserted_puzzle_announcements: HashSet<Bytes32>,
    vault_nonces: HashSet<usize>,
}

impl Facts {
    pub fn extend(&mut self, other: &Facts) -> Result<(), DriverError> {
        if let Some(time) = other.actual_expiration_time {
            self.update_actual_expiration_time(time);
        }

        if let Some(time) = other.required_expiration_time {
            self.update_required_expiration_time(time);
        }

        self.reserved_fees += other.reserved_fees;

        self.coin_spends.extend(other.coin_spends.clone());

        self.requested_payments
            .extend(other.requested_payments.clone());

        self.asset_info.extend(other.asset_info.clone())?;

        self.p2_puzzles.extend(other.p2_puzzles.clone());

        self.asserted_puzzle_announcements
            .extend(other.asserted_puzzle_announcements.clone());

        self.vault_nonces.extend(other.vault_nonces.clone());

        Ok(())
    }

    pub fn actual_expiration_time(&self) -> Option<u64> {
        self.actual_expiration_time
    }

    pub fn required_expiration_time(&self) -> Option<u64> {
        self.required_expiration_time
    }

    pub fn reserved_fees(&self) -> u128 {
        self.reserved_fees
    }

    pub fn coin_spend(&self, coin_id: Bytes32) -> Option<&RevealedCoinSpend> {
        self.coin_spends.get(&coin_id)
    }

    pub fn requested_payments(&self) -> &RequestedPayments {
        &self.requested_payments
    }

    pub fn asset_info(&self) -> &AssetInfo {
        &self.asset_info
    }

    pub fn p2_puzzle(&self, puzzle_hash: Bytes32) -> Option<&RevealedP2Puzzle> {
        self.p2_puzzles.get(&puzzle_hash.into())
    }

    pub fn is_puzzle_announcement_asserted(&self, announcement_id: Bytes32) -> bool {
        self.asserted_puzzle_announcements
            .contains(&announcement_id)
    }

    pub fn vault_nonces(&self) -> impl Iterator<Item = usize> {
        self.vault_nonces.iter().copied()
    }

    /// All coins that are sent messages from the primary vault (the one being signed for) in the transaction
    /// must be revealed. The coin spend is used to determine both the conditions that the spends output, and
    /// the type of asset being sent.
    ///
    /// In some cases, it's insufficient to only reveal the coin spend. For example, if it's a clawback coin,
    /// you must reveal the clawback itself as well. Otherwise, there's no way to verify if the coin won't
    /// consume the message while doing something other than the delegated puzzle's conditions you expect.
    ///
    /// This also records requested payments (i.e., coin spends with a parent coin id of 32 zeros), which are
    /// used to determine what would be paid to us if the announcement from the settlement puzzle were to be
    /// asserted. Note that requested payments are ignored if they aren't asserted.
    pub fn reveal_coin_spend(
        &mut self,
        allocator: &mut Allocator,
        coin_spend: &CoinSpend,
    ) -> Result<(), DriverError> {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
        let puzzle = Puzzle::parse(allocator, puzzle);
        let solution = coin_spend.solution.to_clvm(allocator)?;

        // If the coin spend's puzzle doesn't match the coin's puzzle hash, we should return an error.
        // This prevents spoofing what will happen as a result of the coin spend being included in the transaction.
        if coin_spend.coin.puzzle_hash != puzzle.curried_puzzle_hash().into() {
            return Err(DriverError::WrongPuzzleHash);
        }

        if coin_spend.coin.parent_coin_info == Bytes32::default() {
            // We can throw away asset info here, since we're not interested in taking the offer.
            self.requested_payments
                .parse(allocator, &mut self.asset_info, puzzle, solution)?;
        }

        self.coin_spends.insert(
            coin_spend.coin.coin_id(),
            RevealedCoinSpend {
                coin: coin_spend.coin,
                puzzle,
                solution,
            },
        );

        Ok(())
    }

    /// Reveals a clawback, so that we can look it up by p2 puzzle hash.
    pub fn reveal_clawback(&mut self, clawback: ClawbackV2) {
        self.p2_puzzles
            .insert(clawback.tree_hash(), RevealedP2Puzzle::Clawback(clawback));
    }

    /// Reveals a list of conditions, so that we can look it up by p2 puzzle hash. The conditions are quoted
    /// and tree hashed to get a valid p2 puzzle hash that would certainly output the conditions if spent.
    pub fn reveal_delegated_conditions(
        &mut self,
        allocator: &mut Allocator,
        conditions: Vec<Condition>,
    ) -> Result<(), DriverError> {
        let p2_puzzle = clvm_quote!(&conditions).to_clvm(allocator)?;
        let p2_puzzle_hash = tree_hash(allocator, p2_puzzle);
        self.p2_puzzles.insert(
            p2_puzzle_hash,
            RevealedP2Puzzle::DelegatedConditions(conditions),
        );
        Ok(())
    }

    /// Updates the transaction's expiration time to the minimum of the current expiration time and
    /// the given time. This is used to ensure that the transaction will not be valid after the given
    /// time (i.e., after a clawback expires).
    pub fn update_actual_expiration_time(&mut self, expiration_time: u64) {
        if let Some(old_time) = self.actual_expiration_time {
            self.actual_expiration_time = Some(min(old_time, expiration_time));
        } else {
            self.actual_expiration_time = Some(expiration_time);
        }
    }

    /// Updates the required expiration time to the minimum of the current required expiration time and
    /// the given time. This is used to ensure that the transaction will not be valid after the given
    /// time (i.e., after a clawback expires).
    pub fn update_required_expiration_time(&mut self, required_expiration_time: u64) {
        if let Some(old_time) = self.required_expiration_time {
            self.required_expiration_time = Some(min(old_time, required_expiration_time));
        } else {
            self.required_expiration_time = Some(required_expiration_time);
        }
    }

    /// Adds to the total reserved fees, from coins that have been validated to be linked.
    pub fn add_reserved_fees(&mut self, amount: u64) {
        self.reserved_fees += u128::from(amount);
    }

    /// Adds an announcement id to the set of asserted puzzle announcements.
    pub fn assert_puzzle_announcement(&mut self, announcement_id: Bytes32) {
        self.asserted_puzzle_announcements.insert(announcement_id);
    }

    /// Adds a vault nonce to the set of vault nonces to derive p2 puzzle hashes for.
    pub fn reveal_vault_nonce(&mut self, nonce: usize) {
        self.vault_nonces.insert(nonce);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
