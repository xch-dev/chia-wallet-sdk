use std::collections::HashMap;

use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
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
use chia_sdk_types::{
    run_puzzle, Conditions, TransferNft, P2_DELEGATED_CONDITIONS_PUZZLE,
    P2_DELEGATED_CONDITIONS_PUZZLE_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{tree_hash, TreeHash};
use clvmr::{serde::node_from_bytes, Allocator, NodePtr};

use crate::{DriverError, Nft, Spend, StandardLayer};

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

    /// Allocate the standard puzzle and return its pointer.
    pub fn standard_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(STANDARD_PUZZLE_HASH, &STANDARD_PUZZLE)
    }

    /// Allocate the CAT puzzle and return its pointer.
    pub fn cat_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(CAT_PUZZLE_HASH, &CAT_PUZZLE)
    }

    /// Allocate the DID inner puzzle and return its pointer.
    pub fn did_inner_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(DID_INNER_PUZZLE_HASH, &DID_INNER_PUZZLE)
    }

    /// Allocate the NFT intermediate launcher puzzle and return its pointer.
    pub fn nft_intermediate_launcher(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(
            NFT_INTERMEDIATE_LAUNCHER_PUZZLE_HASH,
            &NFT_INTERMEDIATE_LAUNCHER_PUZZLE,
        )
    }

    /// Allocate the NFT royalty transfer puzzle and return its pointer.
    pub fn nft_royalty_transfer(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(
            NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
            &NFT_ROYALTY_TRANSFER_PUZZLE,
        )
    }

    /// Allocate the NFT ownership layer puzzle and return its pointer.
    pub fn nft_ownership_layer(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(NFT_OWNERSHIP_LAYER_PUZZLE_HASH, &NFT_OWNERSHIP_LAYER_PUZZLE)
    }

    /// Allocate the NFT state layer puzzle and return its pointer.
    pub fn nft_state_layer(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(NFT_STATE_LAYER_PUZZLE_HASH, &NFT_STATE_LAYER_PUZZLE)
    }

    /// Allocate the singleton top layer puzzle and return its pointer.
    pub fn singleton_top_layer(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(SINGLETON_TOP_LAYER_PUZZLE_HASH, &SINGLETON_TOP_LAYER_PUZZLE)
    }

    /// Allocate the singleton launcher puzzle and return its pointer.
    pub fn singleton_launcher(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(SINGLETON_LAUNCHER_PUZZLE_HASH, &SINGLETON_LAUNCHER_PUZZLE)
    }

    /// Allocate the multi-issuance TAIL puzzle and return its pointer.
    pub fn everything_with_signature_tail_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(
            EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE_HASH,
            &EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE,
        )
    }

    /// Allocate the single-issuance TAIL puzzle and return its pointer.
    pub fn genesis_by_coin_id_tail_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(
            GENESIS_BY_COIN_ID_TAIL_PUZZLE_HASH,
            &GENESIS_BY_COIN_ID_TAIL_PUZZLE,
        )
    }

    /// Allocate the settlement payments puzzle and return its pointer.
    pub fn settlement_payments_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(SETTLEMENT_PAYMENTS_PUZZLE_HASH, &SETTLEMENT_PAYMENTS_PUZZLE)
    }

    /// Allocate the p2 delegated conditions puzzle and return its pointer.
    pub fn p2_delegated_conditions_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(
            P2_DELEGATED_CONDITIONS_PUZZLE_HASH,
            &P2_DELEGATED_CONDITIONS_PUZZLE,
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

    /// Spend a standard p2 coin.
    pub fn spend_standard_coin(
        &mut self,
        coin: Coin,
        synthetic_key: PublicKey,
        conditions: Conditions,
    ) -> Result<(), DriverError> {
        let p2_spend = StandardLayer::new(synthetic_key).spend(self, conditions)?;
        self.spend(coin, p2_spend)
    }

    /// Spend an NFT coin with a standard p2 inner puzzle.
    pub fn spend_standard_nft<M>(
        &mut self,
        nft: Nft<M>,
        synthetic_key: PublicKey,
        p2_puzzle_hash: Bytes32,
        new_nft_owner: Option<TransferNft>,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Nft<M>), DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        if let Some(new_nft_owner) = new_nft_owner {
            let (cs, conds, new_nft) = nft.transfer_to_did(
                self,
                synthetic_key,
                p2_puzzle_hash,
                &new_nft_owner,
                extra_conditions,
            )?;

            self.insert(cs);
            return Ok((conds, new_nft));
        }

        let (cs, new_nft) = nft.transfer(self, synthetic_key, p2_puzzle_hash, extra_conditions)?;

        self.insert(cs);
        Ok((Conditions::new(), new_nft))
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
