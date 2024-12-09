use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_sdk_types::BlsMember;
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::DriverError;

use super::MofN;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MemberMemo<T> {
    pub curried_puzzle_hash: Bytes32,
    pub member: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BlsMemberMemo {
    pub public_key: PublicKey,
}

#[derive(Debug, Clone)]
pub enum Member {
    Bls(BlsMember),
    // do this last
    MofN(MofN),
}

impl Member {
    pub fn from_memo(allocator: &Allocator, memo: NodePtr) -> Result<Self, DriverError> {
        todo!()
    }
}
