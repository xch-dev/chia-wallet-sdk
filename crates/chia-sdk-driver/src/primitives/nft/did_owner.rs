use chia_protocol::Bytes32;
use clvm_traits::ToClvm;
use clvmr::Allocator;

use crate::{DidInfo, DriverError};

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

    pub fn from_did_info<M>(
        allocator: &mut Allocator,
        did_info: &DidInfo<M>,
    ) -> Result<Self, DriverError>
    where
        M: ToClvm<Allocator>,
    {
        Ok(Self {
            did_id: did_info.launcher_id,
            inner_puzzle_hash: did_info.inner_puzzle_hash(allocator)?.into(),
        })
    }
}
