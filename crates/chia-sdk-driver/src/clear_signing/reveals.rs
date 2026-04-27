use std::collections::{HashMap, HashSet};

use chia_consensus::opcodes::RECEIVE_MESSAGE;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_sdk_types::{Condition, Mod, puzzles::SingletonMember};
use clvm_traits::{FromClvm, ToClvm, clvm_quote};
use clvm_utils::{ToTreeHash, TreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};
use num_bigint::BigInt;

use crate::{
    AssetInfo, ClawbackV2, DriverError, MofN, Puzzle, RequestedPayments, mips_puzzle_hash,
};

#[derive(Debug, Clone)]
pub enum RevealedP2Puzzle {
    Clawback(ClawbackV2),
    P2ConditionsOrSingleton(P2ConditionsOrSingletonReveal),
}

#[derive(Debug, Clone)]
pub struct P2ConditionsOrSingletonReveal {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    pub fixed_conditions: Vec<Condition>,
}

#[derive(Debug, Clone, Copy)]
pub struct RevealedCoinSpend {
    pub coin: Coin,
    pub puzzle: Puzzle,
    pub solution: NodePtr,
}

#[derive(Debug, Default, Clone)]
pub struct Reveals {
    coin_spends: HashMap<Bytes32, RevealedCoinSpend>,
    p2_puzzles: HashMap<TreeHash, RevealedP2Puzzle>,
    requested_payments: RequestedPayments,
    asset_info: AssetInfo,
    vault_nonces: HashSet<usize>,
}

impl Reveals {
    pub fn from_spends(
        allocator: &mut Allocator,
        coin_spends: Vec<CoinSpend>,
        spent_clawbacks: Vec<ClawbackV2>,
    ) -> Result<Self, DriverError> {
        let mut reveals = Self::default();

        reveals.reveal_vault_nonce(0);

        for coin_spend in coin_spends {
            reveals.reveal_coin_spend(allocator, &coin_spend)?;
        }

        for clawback in spent_clawbacks {
            reveals.reveal_clawback(clawback);
        }

        Ok(reveals)
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

    /// Reveals a p2 conditions or singleton puzzle, so that we can look it up by p2 puzzle hash.
    pub fn reveal_p2_conditions_or_singleton(
        &mut self,
        allocator: &mut Allocator,
        launcher_id: Bytes32,
        nonce: usize,
        fixed_conditions: Vec<Condition>,
    ) -> Result<(), DriverError> {
        for condition in &fixed_conditions {
            match condition {
                Condition::ReceiveMessage(_) => {
                    return Err(DriverError::ReceiveMessageConditionsNotAllowed);
                }
                Condition::Other(condition) => {
                    let (opcode, _) = <(BigInt, NodePtr)>::from_clvm(allocator, *condition)?;

                    if opcode == BigInt::from(RECEIVE_MESSAGE) {
                        return Err(DriverError::ReceiveMessageConditionsNotAllowed);
                    }
                }
                _ => {}
            }
        }

        let puzzle = clvm_quote!(&fixed_conditions).to_clvm(allocator)?;
        let delegated_spend_hash = tree_hash(allocator, puzzle);

        let fixed_conditions_hash = mips_puzzle_hash(0, vec![], delegated_spend_hash, false);
        let p2_singleton_hash = mips_puzzle_hash(
            nonce,
            vec![],
            SingletonMember::new(launcher_id).curry_tree_hash(),
            false,
        );

        let p2_puzzle_hash = mips_puzzle_hash(
            0,
            vec![],
            MofN::new(1, vec![fixed_conditions_hash, p2_singleton_hash]).inner_puzzle_hash(),
            true,
        );

        self.p2_puzzles.insert(
            p2_puzzle_hash,
            RevealedP2Puzzle::P2ConditionsOrSingleton(P2ConditionsOrSingletonReveal {
                launcher_id,
                nonce,
                fixed_conditions,
            }),
        );

        Ok(())
    }

    /// Adds a vault nonce to the set of vault nonces to derive p2 puzzle hashes for.
    pub fn reveal_vault_nonce(&mut self, nonce: usize) {
        self.vault_nonces.insert(nonce);
    }

    pub fn requested_payments(&self) -> &RequestedPayments {
        &self.requested_payments
    }

    pub fn asset_info(&self) -> &AssetInfo {
        &self.asset_info
    }

    pub fn coin_spends(&self) -> impl Iterator<Item = &RevealedCoinSpend> {
        self.coin_spends.values()
    }

    pub fn coin_spend(&self, coin_id: Bytes32) -> Option<&RevealedCoinSpend> {
        self.coin_spends.get(&coin_id)
    }

    pub fn p2_puzzle(&self, puzzle_hash: TreeHash) -> Option<&RevealedP2Puzzle> {
        self.p2_puzzles.get(&puzzle_hash)
    }

    pub fn vault_nonces(&self) -> impl Iterator<Item = usize> {
        self.vault_nonces.iter().copied()
    }
}
