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
    standard::{StandardArgs, STANDARD_PUZZLE, STANDARD_PUZZLE_HASH},
    Proof,
};
use chia_sdk_types::{conditions::NewNftOwner, puzzles::DidInfo};
use clvm_traits::{FromClvm, FromNodePtr, ToClvm, ToNodePtr};
use clvm_utils::{tree_hash, ToTreeHash, TreeHash};
use clvmr::{run_program, serde::node_from_bytes, Allocator, ChiaDialect, NodePtr};

use crate::{spend_error::SpendError, Conditions, DriverError, Spend, DID, NFT};

/// A wrapper around `Allocator` that caches puzzles and simplifies coin spending.
#[derive(Debug, Default)]
pub struct SpendContext {
    allocator: Allocator,
    puzzles: HashMap<TreeHash, NodePtr>,
    coin_spends: Vec<CoinSpend>,
}

impl SpendContext {
    /// Create a new `SpendContext` from an `Allocator` reference.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a reference to the [`Allocator`].
    pub fn allocator(&self) -> &Allocator {
        &self.allocator
    }

    /// Get a mutable reference to the [`Allocator`].
    pub fn allocator_mut(&mut self) -> &mut Allocator {
        &mut self.allocator
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
    pub fn insert_coin_spend(&mut self, coin_spend: CoinSpend) {
        self.coin_spends.push(coin_spend);
    }

    /// Serializes a [`Spend`] and adds it to the list of coin spends.
    pub fn spend(&mut self, coin: Coin, spend: Spend) -> Result<(), SpendError> {
        let puzzle_reveal = self.serialize(&spend.puzzle())?;
        let solution = self.serialize(&spend.solution())?;
        self.insert_coin_spend(CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }

    /// Allocate a new node and return its pointer.
    pub fn alloc<T>(&mut self, value: &T) -> Result<NodePtr, SpendError>
    where
        T: ToNodePtr,
    {
        Ok(value.to_node_ptr(&mut self.allocator)?)
    }

    /// Extract a value from a node pointer.
    pub fn extract<T>(&self, ptr: NodePtr) -> Result<T, SpendError>
    where
        T: FromNodePtr,
    {
        Ok(T::from_node_ptr(&self.allocator, ptr)?)
    }

    /// Compute the tree hash of a node pointer.
    pub fn tree_hash(&self, ptr: NodePtr) -> TreeHash {
        tree_hash(&self.allocator, ptr)
    }

    /// Run a puzzle with a solution and return the result.
    pub fn run(&mut self, puzzle: NodePtr, solution: NodePtr) -> Result<NodePtr, SpendError> {
        let result = run_program(
            &mut self.allocator,
            &ChiaDialect::new(0),
            puzzle,
            solution,
            u64::MAX,
        )?;
        Ok(result.1)
    }

    /// Serialize a value and return a `Program`.
    pub fn serialize<T>(&mut self, value: &T) -> Result<Program, SpendError>
    where
        T: ToNodePtr,
    {
        let ptr = value.to_node_ptr(&mut self.allocator)?;
        Ok(Program::from_node_ptr(&self.allocator, ptr)?)
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

    /// Allocate the multi-issuance TAIL puzzle and return its pointer.
    pub fn everything_with_signature_tail_puzzle(&mut self) -> Result<NodePtr, SpendError> {
        self.puzzle(
            EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE_HASH,
            &EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE,
        )
    }

    /// Allocate the single-issuance TAIL puzzle and return its pointer.
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
            let puzzle = node_from_bytes(&mut self.allocator, puzzle_bytes)?;
            self.puzzles.insert(puzzle_hash, puzzle);
            Ok(puzzle)
        }
    }

    /// Spend a standard p2 coin.
    pub fn spend_p2_coin(
        &mut self,
        coin: Coin,
        synthetic_key: PublicKey,
        conditions: Conditions,
    ) -> Result<(), SpendError> {
        let p2_spend = conditions.p2_spend(self, synthetic_key)?;
        self.spend(coin, p2_spend)
    }

    /// Spend a DID coin with a standard p2 inner puzzle.
    pub fn spend_standard_did<M>(
        &mut self,
        did: DID<M>,
        lineage_proof: Proof,
        synthetic_key: PublicKey,
        extra_conditions: Conditions,
    ) -> Result<(DID<M>, Proof), DriverError>
    where
        M: ToClvm<NodePtr> + FromClvm<NodePtr> + Clone + ToTreeHash,
    {
        let p2_spend = extra_conditions
            .create_hinted_coin(
                // DID layer does not automatically wrap CREATE_COINs
                did.compute_new_did_layer_puzzle_hash(StandardArgs::curry_tree_hash(synthetic_key))
                    .into(),
                did.coin.amount,
                did.p2_puzzle_hash.into(),
            )
            .p2_spend(self, synthetic_key)?;

        let (did_spend, new_did, new_proof) = did.spend(self, lineage_proof, p2_spend)?;
        self.insert_coin_spend(did_spend);

        Ok((new_did, new_proof))
    }

    /// Spend an NFT coin with a standard p2 inner puzzle.
    pub fn spend_standard_nft<M>(
        &mut self,
        nft: &NFT<M>,
        lineage_proof: Proof,
        synthetic_key: PublicKey,
        p2_puzzle_hash: Bytes32,
        new_nft_owner: Option<NewNftOwner>,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, NFT<M>, Proof), DriverError>
    where
        M: ToClvm<NodePtr> + FromClvm<NodePtr> + Clone + ToTreeHash,
    {
        match new_nft_owner {
            Some(new_nft_owner) => {
                let (cs, conds, new_nft, lp) = nft.transfer_to_did(
                    self,
                    lineage_proof,
                    synthetic_key,
                    p2_puzzle_hash,
                    new_nft_owner,
                    extra_conditions,
                )?;

                self.insert_coin_spend(cs);
                Ok((conds, new_nft, lp))
            }
            None => {
                let (cs, new_nft, lp) = nft.transfer(
                    self,
                    lineage_proof,
                    synthetic_key,
                    p2_puzzle_hash,
                    extra_conditions,
                )?;

                self.insert_coin_spend(cs);
                Ok((Conditions::new(), new_nft, lp))
            }
        }
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

impl From<SpendContext> for Allocator {
    fn from(ctx: SpendContext) -> Self {
        ctx.allocator
    }
}
