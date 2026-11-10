use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_sdk_types::{Conditions, Mod, conditions::Memos, run_puzzle};
use clvm_traits::{FromClvm, ToClvm, clvm_quote};
use clvm_utils::{CurriedProgram, TreeHash, tree_hash};
use clvmr::{
    Allocator, NodePtr,
    allocator::Checkpoint,
    serde::{node_from_bytes, node_to_bytes, node_to_bytes_backrefs},
};

use crate::{DriverError, HashedPtr, Spend};

/// A wrapper around [`Allocator`] that caches puzzles and keeps track of a list of [`CoinSpend`].
/// It's used to construct spend bundles in an easy and efficient way.
///
/// Almost every driver takes a `&mut SpendContext`, since constructing puzzles and solutions
/// requires allocating CLVM values. The context dereferences to an [`Allocator`], so it can be
/// passed anywhere an allocator is expected.
///
/// Puzzle mods (such as the standard puzzle or the CAT puzzle) are deserialized once and cached
/// by their tree hash, so constructing many puzzles with the same mod is cheap. Coin spends are
/// collected as they're created, and can be removed with [`SpendContext::take`] to sign and
/// submit them.
///
/// # Example
///
/// ```
/// use chia_sdk_driver::{SpendContext, StandardLayer};
/// use chia_sdk_test::Simulator;
/// use chia_sdk_types::Conditions;
/// use chia_puzzle_types::{Memos, standard::StandardArgs};
///
/// let mut sim = Simulator::new();
/// let mut ctx = SpendContext::new();
///
/// // Allocate CLVM values, and convert them back
/// let ptr = ctx.alloc(&(42, "hello"))?;
/// let value: (u64, String) = ctx.extract(ptr)?;
/// assert_eq!(value, (42, "hello".to_string()));
///
/// // Curry a puzzle, whose mod is cached by the context
/// let alice = sim.bls(1_000);
/// let puzzle = ctx.curry(StandardArgs::new(alice.pk))?;
/// assert_eq!(ctx.tree_hash(puzzle), alice.puzzle_hash.into());
///
/// // Drivers add coin spends to the context, which can then be taken and submitted
/// let memos = ctx.hint(alice.puzzle_hash)?;
/// StandardLayer::new(alice.pk).spend(
///     &mut ctx,
///     alice.coin,
///     Conditions::new().create_coin(alice.puzzle_hash, 1_000, memos),
/// )?;
///
/// let coin_spends = ctx.take();
/// assert_eq!(coin_spends.len(), 1);
/// sim.spend_coins(coin_spends, &[alice.sk])?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Default)]
pub struct SpendContext {
    allocator: Allocator,
    puzzles: HashMap<TreeHash, NodePtr>,
    coin_spends: Vec<CoinSpend>,
}

impl SpendContext {
    /// Creates an empty context with a new [`Allocator`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Restores the allocator to a [`Checkpoint`], and clears the puzzle cache and the coin spends.
    ///
    /// The checkpoint is created with [`Allocator::checkpoint`]. This frees the memory used since
    /// then, so that a long-lived context doesn't keep growing. The puzzle cache is cleared,
    /// since the cached puzzles may have been allocated after the checkpoint.
    pub fn reset(&mut self, checkpoint: &Checkpoint) {
        self.allocator.restore_checkpoint(checkpoint);
        self.puzzles.clear();
        self.coin_spends.clear();
    }

    /// Iterates over the [`CoinSpend`] that have been collected so far.
    pub fn iter(&self) -> impl Iterator<Item = &CoinSpend> {
        self.coin_spends.iter()
    }

    /// Removes and returns all of the [`CoinSpend`] that have been collected so far.
    ///
    /// The allocator and puzzle cache are kept, so the context can be reused for the next
    /// transaction.
    pub fn take(&mut self) -> Vec<CoinSpend> {
        std::mem::take(&mut self.coin_spends)
    }

    /// Adds a [`CoinSpend`] to the end of the collection.
    pub fn insert(&mut self, coin_spend: CoinSpend) {
        self.coin_spends.push(coin_spend);
    }

    /// Serializes a [`Spend`] of the coin, and adds it to the list of [`CoinSpend`].
    pub fn spend(&mut self, coin: Coin, spend: Spend) -> Result<(), DriverError> {
        let puzzle_reveal = self.serialize(&spend.puzzle)?;
        let solution = self.serialize(&spend.solution)?;
        self.insert(CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }

    /// Allocates a value and returns its pointer.
    pub fn alloc<T>(&mut self, value: &T) -> Result<NodePtr, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        Ok(value.to_clvm(&mut self.allocator)?)
    }

    /// Allocates a value and returns its pointer, along with its tree hash.
    pub fn alloc_hashed<T>(&mut self, value: &T) -> Result<HashedPtr, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        let ptr = value.to_clvm(&mut self.allocator)?;
        Ok(HashedPtr::from_ptr(self, ptr))
    }

    /// Converts the CLVM value at a pointer into a Rust value.
    pub fn extract<T>(&self, ptr: NodePtr) -> Result<T, DriverError>
    where
        T: FromClvm<Allocator>,
    {
        Ok(T::from_clvm(&self.allocator, ptr)?)
    }

    /// Calculates the tree hash of the value at a pointer.
    pub fn tree_hash(&self, ptr: NodePtr) -> TreeHash {
        tree_hash(&self.allocator, ptr)
    }

    /// Runs a puzzle with a solution, and returns the output.
    ///
    /// The cost is limited to the maximum CLVM cost of a mainnet block. For a coin's puzzle, the
    /// output is its list of conditions.
    pub fn run(&mut self, puzzle: NodePtr, solution: NodePtr) -> Result<NodePtr, DriverError> {
        Ok(run_puzzle(&mut self.allocator, puzzle, solution)?)
    }

    /// Allocates a value and serializes it into a [`Program`].
    pub fn serialize<T>(&mut self, value: &T) -> Result<Program, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        let ptr = value.to_clvm(&mut self.allocator)?;
        Ok(node_to_bytes(&self.allocator, ptr)?.into())
    }

    /// Allocates a value and serializes it into a [`Program`], compressing repeated subtrees
    /// with back references.
    pub fn serialize_with_backrefs<T>(&mut self, value: &T) -> Result<Program, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        let ptr = value.to_clvm(&mut self.allocator)?;
        Ok(node_to_bytes_backrefs(&self.allocator, ptr)?.into())
    }

    /// Allocates a value to use as the memos of a `CREATE_COIN` condition.
    pub fn memos<T>(&mut self, value: &T) -> Result<Memos<NodePtr>, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        Ok(Memos::Some(self.alloc(value)?))
    }

    /// Creates memos which only contain a hint, which is typically the p2 puzzle hash of the
    /// coin being created so that wallets can find it.
    pub fn hint(&mut self, hint: Bytes32) -> Result<Memos<NodePtr>, DriverError> {
        self.memos(&[hint])
    }

    /// Allocates the mod of the puzzle `T`, or returns the cached pointer if it was already
    /// allocated.
    pub fn alloc_mod<T>(&mut self) -> Result<NodePtr, DriverError>
    where
        T: Mod,
    {
        self.puzzle(T::mod_hash(), T::mod_reveal().as_ref())
    }

    /// Curries the arguments into the mod that they belong to, and returns the full puzzle.
    ///
    /// The mod is cached, so only the curried arguments are allocated each time.
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

    /// Returns the cached puzzle with the given tree hash, if it has been allocated.
    pub fn get_puzzle(&self, puzzle_hash: &TreeHash) -> Option<NodePtr> {
        self.puzzles.get(puzzle_hash).copied()
    }

    /// Deserializes and caches a puzzle with the given tree hash, or returns the cached pointer
    /// if it was already allocated.
    ///
    /// The tree hash isn't checked against the puzzle bytes, so it must be correct.
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

    /// Creates a [`Spend`] whose puzzle is the quoted conditions and whose solution is nil.
    ///
    /// Running the puzzle outputs the conditions, so it can be used as a delegated puzzle, for
    /// example with [`StandardLayer::delegated_inner_spend`](crate::StandardLayer::delegated_inner_spend).
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
