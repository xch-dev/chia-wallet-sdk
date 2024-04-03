use std::collections::HashMap;

use chia_protocol::{Bytes32, Program};
use chia_wallet::{
    cat::{CAT_PUZZLE, CAT_PUZZLE_HASH, EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE},
    standard::{STANDARD_PUZZLE, STANDARD_PUZZLE_HASH},
};
use clvm_traits::{FromClvmError, FromNodePtr, ToClvmError, ToNodePtr};
use clvm_utils::tree_hash;
use clvmr::{serde::node_from_bytes, Allocator, NodePtr};
use hex_literal::hex;
use thiserror::Error;

mod cat;
mod did;
mod nft;
mod standard;

pub use cat::*;
pub use did::*;
pub use nft::*;
pub use standard::*;

/// Errors that can occur when spending a coin.
#[derive(Debug, Error)]
pub enum SpendError {
    /// An error occurred while converting to clvm.
    #[error("to clvm error: {0}")]
    ToClvm(#[from] ToClvmError),

    /// An error occurred while converting from clvm.
    #[error("from clvm error: {0}")]
    FromClvm(#[from] FromClvmError),
}

/// A wrapper around `Allocator` that caches puzzles and simplifies coin spending.
pub struct SpendContext<'a> {
    allocator: &'a mut Allocator,
    puzzles: HashMap<[u8; 32], NodePtr>,
}

impl<'a> SpendContext<'a> {
    /// Create a new `SpendContext` from an `Allocator` reference.
    pub fn new(allocator: &'a mut Allocator) -> Self {
        Self {
            allocator,
            puzzles: HashMap::new(),
        }
    }

    /// Allocate a new node and return its pointer.
    pub fn alloc<T>(&mut self, value: T) -> Result<NodePtr, SpendError>
    where
        T: ToNodePtr,
    {
        Ok(value.to_node_ptr(self.allocator)?)
    }

    /// Extract a value from a node pointer.
    pub fn extract<T>(&self, ptr: NodePtr) -> Result<T, SpendError>
    where
        T: FromNodePtr,
    {
        Ok(T::from_node_ptr(self.allocator, ptr)?)
    }

    /// Compute the tree hash of a node pointer.
    pub fn tree_hash(&self, ptr: NodePtr) -> Bytes32 {
        Bytes32::new(tree_hash(self.allocator, ptr))
    }

    /// Serialize a value and return a `Program`.
    pub fn serialize<T>(&mut self, value: T) -> Result<Program, SpendError>
    where
        T: ToNodePtr,
    {
        let ptr = value.to_node_ptr(self.allocator)?;
        Ok(Program::from_node_ptr(self.allocator, ptr)?)
    }

    /// Allocate the standard puzzle and return its pointer.
    pub fn standard_puzzle(&mut self) -> NodePtr {
        self.puzzle(&STANDARD_PUZZLE_HASH, &STANDARD_PUZZLE)
    }

    /// Allocate the CAT puzzle and return its pointer.
    pub fn cat_puzzle(&mut self) -> NodePtr {
        self.puzzle(&CAT_PUZZLE_HASH, &CAT_PUZZLE)
    }

    /// Allocate the EverythingWithSignature TAIL puzzle and return its pointer.
    pub fn everything_with_signature_tail_puzzle(&mut self) -> NodePtr {
        // todo: add constant to chia_rs
        self.puzzle(
            &hex!("1720d13250a7c16988eaf530331cefa9dd57a76b2c82236bec8bbbff91499b89"),
            &EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE,
        )
    }

    /// Preload a puzzle into the cache.
    pub fn preload(&mut self, puzzle_hash: [u8; 32], ptr: NodePtr) {
        self.puzzles.insert(puzzle_hash, ptr);
    }

    /// Get a puzzle from the cache or allocate a new one.
    pub fn puzzle(&mut self, puzzle_hash: &[u8; 32], puzzle_bytes: &[u8]) -> NodePtr {
        if let Some(puzzle) = self.puzzles.get(puzzle_bytes) {
            *puzzle
        } else {
            let puzzle = node_from_bytes(self.allocator, puzzle_bytes).unwrap();
            self.puzzles.insert(*puzzle_hash, puzzle);
            puzzle
        }
    }
}
