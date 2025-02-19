use std::collections::HashMap;

use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::{
    NFT_METADATA_UPDATER_DEFAULT, NFT_METADATA_UPDATER_DEFAULT_HASH, SETTLEMENT_PAYMENT,
    SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER, SINGLETON_LAUNCHER_HASH,
};
use chia_sdk_types::{run_puzzle, Conditions, Memos, Mod};
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvm_utils::{tree_hash, CurriedProgram, TreeHash};
use clvmr::{serde::node_from_bytes, Allocator, NodePtr};

#[cfg(feature = "experimental-vaults")]
use chia_sdk_types::{
    FORCE_ASSERT_COIN_ANNOUNCEMENT_PUZZLE, FORCE_ASSERT_COIN_ANNOUNCEMENT_PUZZLE_HASH,
    FORCE_COIN_MESSAGE_PUZZLE, FORCE_COIN_MESSAGE_PUZZLE_HASH,
    PREVENT_MULTIPLE_CREATE_COINS_PUZZLE, PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH,
};

use crate::{DriverError, Spend};

/// A wrapper around [`Allocator`] that caches puzzles and keeps track of a list of [`CoinSpend`].
/// It's used to construct spend bundles in an easy and efficient way.
#[derive(Debug, Default)]
pub struct SpendContext {
    pub allocator: Allocator,
    puzzles: HashMap<TreeHash, NodePtr>,
    coin_spends: Vec<CoinSpend>,
}

impl SpendContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = &CoinSpend> {
        self.coin_spends.iter()
    }

    /// Remove all of the [`CoinSpend`] that have been collected so far.
    pub fn take(&mut self) -> Vec<CoinSpend> {
        std::mem::take(&mut self.coin_spends)
    }

    /// Adds a [`CoinSpend`] to the collection.
    pub fn insert(&mut self, coin_spend: CoinSpend) {
        self.coin_spends.push(coin_spend);
    }

    /// Serializes a [`Spend`] and adds it to the list of [`CoinSpend`].
    pub fn spend(&mut self, coin: Coin, spend: Spend) -> Result<(), DriverError> {
        let puzzle_reveal = self.serialize(&spend.puzzle)?;
        let solution = self.serialize(&spend.solution)?;
        self.insert(CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }

    /// Allocate a new node and return its pointer.
    pub fn alloc<T>(&mut self, value: &T) -> Result<NodePtr, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        Ok(value.to_clvm(&mut self.allocator)?)
    }

    /// Extract a value from a node pointer.
    pub fn extract<T>(&self, ptr: NodePtr) -> Result<T, DriverError>
    where
        T: FromClvm<Allocator>,
    {
        Ok(T::from_clvm(&self.allocator, ptr)?)
    }

    /// Compute the tree hash of a node pointer.
    pub fn tree_hash(&self, ptr: NodePtr) -> TreeHash {
        tree_hash(&self.allocator, ptr)
    }

    /// Run a puzzle with a solution and return the result.
    pub fn run(&mut self, puzzle: NodePtr, solution: NodePtr) -> Result<NodePtr, DriverError> {
        Ok(run_puzzle(&mut self.allocator, puzzle, solution)?)
    }

    /// Serialize a value and return a `Program`.
    pub fn serialize<T>(&mut self, value: &T) -> Result<Program, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        let ptr = value.to_clvm(&mut self.allocator)?;
        Ok(Program::from_clvm(&self.allocator, ptr)?)
    }

    pub fn memos<T>(&mut self, value: &T) -> Result<Memos<NodePtr>, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        Ok(Memos::new(self.alloc(value)?))
    }

    pub fn hint(&mut self, hint: Bytes32) -> Result<Memos<NodePtr>, DriverError> {
        Ok(Memos::hint(&mut self.allocator, hint)?)
    }

    pub fn alloc_mod<T>(&mut self) -> Result<NodePtr, DriverError>
    where
        T: Mod,
    {
        self.puzzle(T::MOD_HASH, T::MOD_REVEAL)
    }

    pub fn curry<T>(&mut self, args: T) -> Result<NodePtr, DriverError>
    where
        T: Mod + ToClvm<Allocator>,
    {
        let mod_ptr = self.alloc_mod::<T>()?;
        self.alloc(&CurriedProgram {
            program: mod_ptr,
            args,
        })
    }

    pub fn nft_metadata_updater(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(
            NFT_METADATA_UPDATER_DEFAULT_HASH.into(),
            &NFT_METADATA_UPDATER_DEFAULT,
        )
    }

    pub fn singleton_launcher(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(SINGLETON_LAUNCHER_HASH.into(), &SINGLETON_LAUNCHER)
    }

    pub fn settlement_payments_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(SETTLEMENT_PAYMENT_HASH.into(), &SETTLEMENT_PAYMENT)
    }

    #[cfg(feature = "experimental-vaults")]
    pub fn force_coin_message_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(FORCE_COIN_MESSAGE_PUZZLE_HASH, &FORCE_COIN_MESSAGE_PUZZLE)
    }

    #[cfg(feature = "experimental-vaults")]
    pub fn force_assert_coin_announcement_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(
            FORCE_ASSERT_COIN_ANNOUNCEMENT_PUZZLE_HASH,
            &FORCE_ASSERT_COIN_ANNOUNCEMENT_PUZZLE,
        )
    }

    #[cfg(feature = "experimental-vaults")]
    pub fn prevent_multiple_create_coins_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(
            PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH,
            &PREVENT_MULTIPLE_CREATE_COINS_PUZZLE,
        )
    }

    /// Preload a puzzle into the cache.
    pub fn preload(&mut self, puzzle_hash: TreeHash, ptr: NodePtr) {
        self.puzzles.insert(puzzle_hash, ptr);
    }

    /// Checks whether a puzzle is in the cache.
    pub fn get_puzzle(&self, puzzle_hash: &TreeHash) -> Option<NodePtr> {
        self.puzzles.get(puzzle_hash).copied()
    }

    /// Get a puzzle from the cache or allocate a new one.
    pub fn puzzle(
        &mut self,
        puzzle_hash: TreeHash,
        puzzle_bytes: &[u8],
    ) -> Result<NodePtr, DriverError> {
        if let Some(puzzle) = self.puzzles.get(&puzzle_hash) {
            Ok(*puzzle)
        } else {
            let puzzle = node_from_bytes(&mut self.allocator, puzzle_bytes)?;
            self.puzzles.insert(puzzle_hash, puzzle);
            Ok(puzzle)
        }
    }

    pub fn delegated_spend(&mut self, conditions: Conditions) -> Result<Spend, DriverError> {
        let puzzle = self.alloc(&clvm_quote!(conditions))?;
        Ok(Spend::new(puzzle, NodePtr::NIL))
    }
}

impl IntoIterator for SpendContext {
    type Item = CoinSpend;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.coin_spends.into_iter()
    }
}

impl From<Allocator> for SpendContext {
    fn from(allocator: Allocator) -> Self {
        Self {
            allocator,
            puzzles: HashMap::new(),
            coin_spends: Vec::new(),
        }
    }
}
