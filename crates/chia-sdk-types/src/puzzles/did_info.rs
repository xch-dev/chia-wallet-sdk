use chia_protocol::{Bytes32, Coin};
use chia_puzzles::Proof;

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
