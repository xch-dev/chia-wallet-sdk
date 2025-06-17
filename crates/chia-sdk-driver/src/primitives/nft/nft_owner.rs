use chia_protocol::Bytes32;
use clvm_utils::ToTreeHash;

use crate::DidInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NftOwner {
    pub launcher_id: Bytes32,
    pub singleton_inner_puzzle_hash: Bytes32,
}

impl NftOwner {
    pub fn new(launcher_id: Bytes32, singleton_inner_puzzle_hash: Bytes32) -> Self {
        Self {
            launcher_id,
            singleton_inner_puzzle_hash,
        }
    }

    pub fn from_did_info<M>(did_info: &DidInfo<M>) -> Self
    where
        M: ToTreeHash,
    {
        Self {
            launcher_id: did_info.launcher_id,
            singleton_inner_puzzle_hash: did_info.inner_puzzle_hash().into(),
        }
    }
}
