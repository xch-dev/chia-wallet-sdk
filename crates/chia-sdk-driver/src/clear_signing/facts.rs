use std::{
    cmp::min,
    collections::{HashMap, HashSet},
};

use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_sdk_types::Condition;
use clvm_traits::{ToClvm, clvm_quote};
use clvm_utils::{ToTreeHash, TreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{ClawbackV2, DriverError, Puzzle};

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
    expiration_time: u64,
    reserved_fees: u128,
    coin_spends: HashMap<Bytes32, RevealedCoinSpend>,
    p2_puzzles: HashMap<TreeHash, RevealedP2Puzzle>,
    asserted_puzzle_announcements: HashSet<Bytes32>,
}

impl Facts {
    /// All coins that are sent messages from the primary vault (the one being signed for) in the transaction
    /// must be revealed. The coin spend is used to determine both the conditions that the spends output, and
    /// the type of asset being sent.
    ///
    /// In some cases, it's insufficient to only reveal the coin spend. For example, if it's a clawback coin,
    /// you must reveal the clawback itself as well. Otherwise, there's no way to verify if the coin won't
    /// consume the message while doing something other than the delegated puzzle's conditions you expect.
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
    pub fn update_expiration_time(&mut self, expiration_time: u64) {
        self.expiration_time = min(self.expiration_time, expiration_time);
    }

    /// Adds to the total reserved fees, from coins that have been validated to be linked.
    pub fn add_reserved_fees(&mut self, amount: u64) {
        self.reserved_fees += u128::from(amount);
    }

    /// Adds an announcement id to the set of asserted puzzle announcements.
    pub fn assert_puzzle_announcement(&mut self, announcement_id: Bytes32) {
        self.asserted_puzzle_announcements.insert(announcement_id);
    }

    pub fn coin_spend(&self, coin_id: Bytes32) -> Option<&RevealedCoinSpend> {
        self.coin_spends.get(&coin_id)
    }

    pub fn p2_puzzle(&self, puzzle_hash: Bytes32) -> Option<&RevealedP2Puzzle> {
        self.p2_puzzles.get(&puzzle_hash.into())
    }
}
