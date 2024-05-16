use std::collections::HashMap;

use chia_protocol::{CoinSpend, Program};
use chia_puzzles::{
    cat::{
        CAT_PUZZLE, CAT_PUZZLE_HASH, EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE,
        EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE_HASH, GENESIS_BY_COIN_ID_TAIL_PUZZLE,
        GENESIS_BY_COIN_ID_TAIL_PUZZLE_HASH,
    },
    did::{DID_INNER_PUZZLE, DID_INNER_PUZZLE_HASH},
    nft::{
        NFT_INTERMEDIATE_LAUNCHER_PUZZLE, NFT_INTERMEDIATE_LAUNCHER_PUZZLE_HASH,
        NFT_OWNERSHIP_LAYER_PUZZLE, NFT_OWNERSHIP_LAYER_PUZZLE_HASH, NFT_ROYALTY_TRANSFER_PUZZLE,
        NFT_ROYALTY_TRANSFER_PUZZLE_HASH, NFT_STATE_LAYER_PUZZLE, NFT_STATE_LAYER_PUZZLE_HASH,
    },
    offer::{SETTLEMENT_PAYMENTS_PUZZLE, SETTLEMENT_PAYMENTS_PUZZLE_HASH},
    singleton::{
        SINGLETON_LAUNCHER_PUZZLE, SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE,
        SINGLETON_TOP_LAYER_PUZZLE_HASH,
    },
    standard::{STANDARD_PUZZLE, STANDARD_PUZZLE_HASH},
};
use clvm_traits::{FromNodePtr, ToNodePtr};
use clvm_utils::{tree_hash, TreeHash};
use clvmr::{run_program, serde::node_from_bytes, Allocator, ChiaDialect, NodePtr};

use crate::SpendError;

/// A wrapper around `Allocator` that caches puzzles and simplifies coin spending.
pub struct SpendContext<'a> {
    allocator: &'a mut Allocator,
    puzzles: HashMap<TreeHash, NodePtr>,
    coin_spends: Vec<CoinSpend>,
}

impl<'a> SpendContext<'a> {
    /// Create a new `SpendContext` from an `Allocator` reference.
    pub fn new(allocator: &'a mut Allocator) -> Self {
        Self {
            allocator,
            puzzles: HashMap::new(),
            coin_spends: Vec::new(),
        }
    }

    /// Get a reference to the [`Allocator`].
    pub fn allocator(&self) -> &Allocator {
        self.allocator
    }

    /// Get a mutable reference to the [`Allocator`].
    pub fn allocator_mut(&mut self) -> &mut Allocator {
        self.allocator
    }

    /// Get a reference to the list of coin spends.
    pub fn spends(&self) -> &[CoinSpend] {
        &self.coin_spends
    }

    /// Take the coin spends out of the [`SpendContext`].
    pub fn take_spends(&mut self) -> Vec<CoinSpend> {
        std::mem::take(&mut self.coin_spends)
    }

    /// Add a [`CoinSpend`] to the list.
    pub fn spend(&mut self, coin_spend: CoinSpend) {
        self.coin_spends.push(coin_spend);
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
    pub fn tree_hash(&self, ptr: NodePtr) -> TreeHash {
        tree_hash(self.allocator, ptr)
    }

    /// Run a puzzle with a solution and return the result.
    pub fn run(&mut self, puzzle: NodePtr, solution: NodePtr) -> Result<NodePtr, SpendError> {
        let result = run_program(
            self.allocator,
            &ChiaDialect::new(0),
            puzzle,
            solution,
            u64::MAX,
        )?;
        Ok(result.1)
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
    pub fn standard_puzzle(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(STANDARD_PUZZLE_HASH, &STANDARD_PUZZLE)
    }

    /// Allocate the CAT puzzle and return its pointer.
    pub fn cat_puzzle(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(CAT_PUZZLE_HASH, &CAT_PUZZLE)
    }

    /// Allocate the DID inner puzzle and return its pointer.
    pub fn did_inner_puzzle(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(DID_INNER_PUZZLE_HASH, &DID_INNER_PUZZLE)
    }

    /// Allocate the NFT intermediate launcher puzzle and return its pointer.
    pub fn nft_intermediate_launcher(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(
            NFT_INTERMEDIATE_LAUNCHER_PUZZLE_HASH,
            &NFT_INTERMEDIATE_LAUNCHER_PUZZLE,
        )
    }

    /// Allocate the NFT royalty transfer puzzle and return its pointer.
    pub fn nft_royalty_transfer(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(
            NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
            &NFT_ROYALTY_TRANSFER_PUZZLE,
        )
    }

    /// Allocate the NFT ownership layer puzzle and return its pointer.
    pub fn nft_ownership_layer(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(NFT_OWNERSHIP_LAYER_PUZZLE_HASH, &NFT_OWNERSHIP_LAYER_PUZZLE)
    }

    /// Allocate the NFT state layer puzzle and return its pointer.
    pub fn nft_state_layer(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(NFT_STATE_LAYER_PUZZLE_HASH, &NFT_STATE_LAYER_PUZZLE)
    }

    /// Allocate the singleton top layer puzzle and return its pointer.
    pub fn singleton_top_layer(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(SINGLETON_TOP_LAYER_PUZZLE_HASH, &SINGLETON_TOP_LAYER_PUZZLE)
    }

    /// Allocate the singleton launcher puzzle and return its pointer.
    pub fn singleton_launcher(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(SINGLETON_LAUNCHER_PUZZLE_HASH, &SINGLETON_LAUNCHER_PUZZLE)
    }

    /// Allocate the EverythingWithSignature TAIL puzzle and return its pointer.
    pub fn everything_with_signature_tail_puzzle(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(
            EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE_HASH,
            &EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE,
        )
    }

    // Allocate the GenesisByCoinId TAIL puzzle and return its pointer.
    pub fn genesis_by_coin_id_tail_puzzle(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(
            GENESIS_BY_COIN_ID_TAIL_PUZZLE_HASH,
            &GENESIS_BY_COIN_ID_TAIL_PUZZLE,
        )
    }

    /// Allocate the settlement payments puzzle and return its pointer.
    pub fn settlement_payments_puzzle(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(SETTLEMENT_PAYMENTS_PUZZLE_HASH, &SETTLEMENT_PAYMENTS_PUZZLE)
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
    ) -> Result<NodePtr, SpendError> {
        if let Some(puzzle) = self.puzzles.get(&puzzle_hash) {
            Ok(*puzzle)
        } else {
            let puzzle = node_from_bytes(self.allocator, puzzle_bytes)?;
            self.puzzles.insert(puzzle_hash, puzzle);
            Ok(puzzle)
        }
    }
}
