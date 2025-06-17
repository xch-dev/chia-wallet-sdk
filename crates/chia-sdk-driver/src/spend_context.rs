use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_sdk_types::{conditions::Memos, run_puzzle, Conditions, Mod};
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvm_utils::{tree_hash, CurriedProgram, TreeHash};
use clvmr::{
    serde::{node_from_bytes, node_to_bytes, node_to_bytes_backrefs},
    Allocator, NodePtr,
};

use crate::{DriverError, HashedPtr, Spend};

/// A wrapper around [`Allocator`] that caches puzzles and keeps track of a list of [`CoinSpend`].
/// It's used to construct spend bundles in an easy and efficient way.
#[derive(Debug, Default)]
pub struct SpendContext {
    allocator: Allocator,
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

    /// Allocate a new node and return its pointer pre-hashed.
    pub fn alloc_hashed<T>(&mut self, value: &T) -> Result<HashedPtr, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        let ptr = value.to_clvm(&mut self.allocator)?;
        Ok(HashedPtr::from_ptr(self, ptr))
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

    /// Allocate a value and serialize it into a [`Program`].
    pub fn serialize<T>(&mut self, value: &T) -> Result<Program, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        let ptr = value.to_clvm(&mut self.allocator)?;
        Ok(node_to_bytes(&self.allocator, ptr)?.into())
    }

    /// Allocate a value and serialize it into a [`Program`] with back references enabled.
    pub fn serialize_with_backrefs<T>(&mut self, value: &T) -> Result<Program, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        let ptr = value.to_clvm(&mut self.allocator)?;
        Ok(node_to_bytes_backrefs(&self.allocator, ptr)?.into())
    }

    pub fn memos<T>(&mut self, value: &T) -> Result<Memos<NodePtr>, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        Ok(Memos::Some(self.alloc(value)?))
    }

    pub fn hint(&mut self, hint: Bytes32) -> Result<Memos<NodePtr>, DriverError> {
        self.memos(&[hint])
    }

    pub fn alloc_mod<T>(&mut self) -> Result<NodePtr, DriverError>
    where
        T: Mod,
    {
        self.puzzle(T::mod_hash(), T::mod_reveal().as_ref())
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

    pub fn get_puzzle(&self, puzzle_hash: &TreeHash) -> Option<NodePtr> {
        self.puzzles.get(puzzle_hash).copied()
    }

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

impl Deref for SpendContext {
    type Target = Allocator;

    fn deref(&self) -> &Self::Target {
        &self.allocator
    }
}

impl DerefMut for SpendContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.allocator
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
