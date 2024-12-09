use chia_protocol::Bytes32;
use chia_sdk_types::Timelock;
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::DriverError;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RestrictionMemo<T> {
    pub is_morpher: bool,
    pub curried_puzzle_hash: Bytes32,
    pub restriction: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct TimelockMemo {
    pub seconds: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum Restriction {
    Timelock(Timelock),
}

impl Restriction {
    pub fn from_memo(allocator: &Allocator, memo: NodePtr) -> Result<Self, DriverError> {
        let memo = RestrictionMemo::from_clvm(allocator, memo)?;
        let restriction = TimelockMemo::from_clvm(allocator, memo.restriction)?;
        Ok(Self::Timelock(Timelock::new(restriction.seconds)))
    }
}
