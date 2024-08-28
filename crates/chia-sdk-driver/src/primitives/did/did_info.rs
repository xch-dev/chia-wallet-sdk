use chia_protocol::Bytes32;
use chia_puzzles::{did::DidArgs, singleton::SingletonStruct};
use clvm_traits::ToClvm;
use clvm_utils::{tree_hash, ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{DidLayer, DriverError, SingletonLayer};

pub type StandardDidLayers<M, I> = SingletonLayer<DidLayer<M, I>>;

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DidInfo<M> {
    pub launcher_id: Bytes32,
    pub recovery_list_hash: Bytes32,
    pub num_verifications_required: u64,
    pub metadata: M,
    pub p2_puzzle_hash: Bytes32,
}

impl<M> DidInfo<M> {
    pub fn new(
        launcher_id: Bytes32,
        recovery_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            launcher_id,
            recovery_list_hash,
            num_verifications_required,
            metadata,
            p2_puzzle_hash,
        }
    }

    pub fn from_layers<I>(layers: StandardDidLayers<M, I>) -> Self
    where
        I: ToTreeHash,
    {
        Self {
            launcher_id: layers.launcher_id,
            recovery_list_hash: layers.inner_puzzle.recovery_list_hash,
            num_verifications_required: layers.inner_puzzle.num_verifications_required,
            metadata: layers.inner_puzzle.metadata,
            p2_puzzle_hash: layers.inner_puzzle.inner_puzzle.tree_hash().into(),
        }
    }

    #[must_use]
    pub fn into_layers<I>(self, p2_puzzle: I) -> StandardDidLayers<M, I> {
        SingletonLayer::new(
            self.launcher_id,
            DidLayer::new(
                self.launcher_id,
                self.recovery_list_hash,
                self.num_verifications_required,
                self.metadata,
                p2_puzzle,
            ),
        )
    }

    pub fn with_metadata<N>(self, metadata: N) -> DidInfo<N> {
        DidInfo {
            launcher_id: self.launcher_id,
            recovery_list_hash: self.recovery_list_hash,
            num_verifications_required: self.num_verifications_required,
            metadata,
            p2_puzzle_hash: self.p2_puzzle_hash,
        }
    }

    pub fn inner_puzzle_hash(&self, allocator: &mut Allocator) -> Result<TreeHash, DriverError>
    where
        M: ToClvm<Allocator>,
    {
        let metadata_ptr = self.metadata.to_clvm(allocator)?;

        Ok(DidArgs::curry_tree_hash(
            self.p2_puzzle_hash.into(),
            self.recovery_list_hash,
            self.num_verifications_required,
            SingletonStruct::new(self.launcher_id),
            tree_hash(allocator, metadata_ptr),
        ))
    }
}
