use chia_protocol::Bytes32;
use chia_puzzles::{did::DidArgs, singleton::SingletonStruct};
use clvm_utils::{ToTreeHash, TreeHash};

#[derive(Debug, Clone, Copy)]
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

    pub fn inner_puzzle_hash(&self) -> TreeHash
    where
        M: ToTreeHash,
    {
        DidArgs::curry_tree_hash(
            self.p2_puzzle_hash.into(),
            self.recovery_list_hash,
            self.num_verifications_required,
            SingletonStruct::new(self.launcher_id),
            self.metadata.tree_hash(),
        )
    }
}
