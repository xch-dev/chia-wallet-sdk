use chia_protocol::Bytes32;
use clvm_utils::ToTreeHash;

use crate::DidInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DidOwner {
    pub did_id: Bytes32,
    pub inner_puzzle_hash: Bytes32,
}

impl DidOwner {
    pub fn new(did_id: Bytes32, inner_puzzle_hash: Bytes32) -> Self {
        Self {
            did_id,
            inner_puzzle_hash,
        }
    }

    pub fn from_did_info<M>(did_info: &DidInfo<M>) -> Self
    where
        M: ToTreeHash,
    {
        Self {
            did_id: did_info.launcher_id,
            inner_puzzle_hash: did_info.inner_puzzle_hash().into(),
        }
    }
}
