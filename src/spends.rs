use std::collections::HashMap;

use chia_protocol::{Bytes32, Program, SpendBundle};
use chia_wallet::{
    cat::{CAT_PUZZLE, CAT_PUZZLE_HASH, EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE},
    did::{DID_INNER_PUZZLE, DID_INNER_PUZZLE_HASH},
    nft::{
        NFT_INTERMEDIATE_LAUNCHER_PUZZLE, NFT_INTERMEDIATE_LAUNCHER_PUZZLE_HASH,
        NFT_OWNERSHIP_LAYER_PUZZLE, NFT_OWNERSHIP_LAYER_PUZZLE_HASH, NFT_ROYALTY_TRANSFER_PUZZLE,
        NFT_ROYALTY_TRANSFER_PUZZLE_HASH, NFT_STATE_LAYER_PUZZLE, NFT_STATE_LAYER_PUZZLE_HASH,
    },
    singleton::{
        SINGLETON_LAUNCHER_PUZZLE, SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE,
        SINGLETON_TOP_LAYER_PUZZLE_HASH,
    },
    standard::{STANDARD_PUZZLE, STANDARD_PUZZLE_HASH},
};
use clvm_traits::{FromClvmError, FromNodePtr, ToClvmError, ToNodePtr};
use clvm_utils::tree_hash;
use clvmr::{
    reduction::EvalErr, run_program, serde::node_from_bytes, Allocator, ChiaDialect, NodePtr,
};
use hex_literal::hex;
use serde::{Deserialize, Serialize};
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

    /// An error occurred while evaluating a program.
    #[error("eval error: {0}")]
    Eval(#[from] EvalErr),
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
    pub fn standard_puzzle(&mut self) -> NodePtr {
        self.puzzle(&STANDARD_PUZZLE_HASH, &STANDARD_PUZZLE)
    }

    /// Allocate the CAT puzzle and return its pointer.
    pub fn cat_puzzle(&mut self) -> NodePtr {
        self.puzzle(&CAT_PUZZLE_HASH, &CAT_PUZZLE)
    }

    /// Allocate the DID inner puzzle and return its pointer.
    pub fn did_inner_puzzle(&mut self) -> NodePtr {
        self.puzzle(&DID_INNER_PUZZLE_HASH, &DID_INNER_PUZZLE)
    }

    /// Allocate the NFT intermediate launcher puzzle and return its pointer.
    pub fn nft_intermediate_launcher(&mut self) -> NodePtr {
        self.puzzle(
            &NFT_INTERMEDIATE_LAUNCHER_PUZZLE_HASH,
            &NFT_INTERMEDIATE_LAUNCHER_PUZZLE,
        )
    }

    /// Allocate the NFT royalty transfer puzzle and return its pointer.
    pub fn nft_royalty_transfer(&mut self) -> NodePtr {
        self.puzzle(
            &NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
            &NFT_ROYALTY_TRANSFER_PUZZLE,
        )
    }

    /// Allocate the NFT ownership layer puzzle and return its pointer.
    pub fn nft_ownership_layer(&mut self) -> NodePtr {
        self.puzzle(
            &NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
            &NFT_OWNERSHIP_LAYER_PUZZLE,
        )
    }

    /// Allocate the NFT state layer puzzle and return its pointer.
    pub fn nft_state_layer(&mut self) -> NodePtr {
        self.puzzle(&NFT_STATE_LAYER_PUZZLE_HASH, &NFT_STATE_LAYER_PUZZLE)
    }

    /// Allocate the singleton top layer puzzle and return its pointer.
    pub fn singleton_top_layer(&mut self) -> NodePtr {
        self.puzzle(
            &SINGLETON_TOP_LAYER_PUZZLE_HASH,
            &SINGLETON_TOP_LAYER_PUZZLE,
        )
    }

    /// Allocate the singleton launcher puzzle and return its pointer.
    pub fn singleton_launcher(&mut self) -> NodePtr {
        self.puzzle(&SINGLETON_LAUNCHER_PUZZLE_HASH, &SINGLETON_LAUNCHER_PUZZLE)
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

#[derive(Serialize, Deserialize)]
struct CoinJson {
    parent_coin_info: String,
    puzzle_hash: String,
    amount: u64,
}

#[derive(Serialize, Deserialize)]
struct CoinSpendJson {
    coin: CoinJson,
    puzzle_reveal: String,
    solution: String,
}

#[derive(Serialize, Deserialize)]
struct SpendBundleJson {
    coin_spends: Vec<CoinSpendJson>,
    aggregated_signature: String,
}

/// Dump a `SpendBundle` to a JSON string.
pub fn dump_spend_bundle(bundle: &SpendBundle) -> String {
    let mut coin_spends = Vec::new();

    for coin_spend in &bundle.coin_spends {
        coin_spends.push(CoinSpendJson {
            coin: CoinJson {
                parent_coin_info: format!("0x{}", hex::encode(coin_spend.coin.parent_coin_info)),
                puzzle_hash: format!("0x{}", hex::encode(coin_spend.coin.puzzle_hash)),
                amount: coin_spend.coin.amount,
            },
            puzzle_reveal: hex::encode(&coin_spend.puzzle_reveal),
            solution: hex::encode(&coin_spend.solution),
        });
    }

    let json = SpendBundleJson {
        coin_spends,
        aggregated_signature: hex::encode(bundle.aggregated_signature.to_bytes()),
    };

    serde_json::to_string(&json).unwrap()
}
