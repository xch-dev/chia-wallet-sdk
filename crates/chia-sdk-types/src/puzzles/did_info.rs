use chia_protocol::{Bytes32, Coin};
use chia_puzzles::{
    did::{DidArgs, DID_INNER_PUZZLE_HASH},
    singleton::{SingletonArgs, SingletonStruct},
    LineageProof, Proof,
};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct DidInfo<M> {
    pub launcher_id: Bytes32,
    pub coin: Coin,
    pub inner_puzzle_hash: Bytes32,
    pub p2_puzzle_hash: Bytes32,
    pub proof: Proof,
    pub recovery_did_list_hash: Bytes32,
    pub num_verifications_required: u64,
    pub metadata: M,
}

impl<M> DidInfo<M>
where
    M: ToTreeHash,
{
    pub fn child(self, p2_puzzle_hash: Bytes32) -> Self {
        let inner_puzzle_hash = CurriedProgram {
            program: DID_INNER_PUZZLE_HASH,
            args: DidArgs {
                inner_puzzle: TreeHash::from(p2_puzzle_hash),
                recovery_did_list_hash: self.recovery_did_list_hash,
                num_verifications_required: self.num_verifications_required,
                singleton_struct: SingletonStruct::new(self.launcher_id),
                metadata: self.metadata.tree_hash(),
            },
        }
        .tree_hash();

        let puzzle_hash = SingletonArgs::curry_tree_hash(self.launcher_id, inner_puzzle_hash);

        Self {
            launcher_id: self.launcher_id,
            coin: Coin::new(self.coin.coin_id(), puzzle_hash.into(), self.coin.amount),
            inner_puzzle_hash: inner_puzzle_hash.into(),
            p2_puzzle_hash,
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_id: self.coin.parent_coin_info,
                parent_inner_puzzle_hash: self.inner_puzzle_hash,
                parent_amount: self.coin.amount,
            }),
            metadata: self.metadata,
            recovery_did_list_hash: self.recovery_did_list_hash,
            num_verifications_required: self.num_verifications_required,
        }
    }
}

impl<M> DidInfo<M> {
    pub fn with_metadata<N>(self, metadata: N) -> DidInfo<N> {
        DidInfo {
            launcher_id: self.launcher_id,
            coin: self.coin,
            inner_puzzle_hash: self.inner_puzzle_hash,
            p2_puzzle_hash: self.p2_puzzle_hash,
            proof: self.proof,
            recovery_did_list_hash: self.recovery_did_list_hash,
            num_verifications_required: self.num_verifications_required,
            metadata,
        }
    }
}
