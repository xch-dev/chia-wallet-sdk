use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::DriverError;

use super::Member;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MofNMemo<T> {
    pub required: usize,
    pub members: Vec<T>,
}

#[derive(Debug, Clone)]
pub struct MofN {
    required: usize,
    members: Vec<Member>,
}

impl MofN {
    pub fn new(required: usize, members: Vec<Member>) -> Option<Self> {
        if members.len() < required {
            return None;
        }
        Some(Self { required, members })
    }

    pub fn from_memo(allocator: &Allocator, memo: NodePtr) -> Result<Self, DriverError> {
        let memo = MofNMemo::from_clvm(allocator, memo)?;

        if memo.members.len() < memo.required {
            return Err(DriverError::InvalidMemo);
        }

        let mut members = Vec::with_capacity(memo.members.len());

        for member_memo in memo.members {
            members.push(Member::from_memo(allocator, member_memo)?);
        }

        Ok(Self {
            required: memo.required,
            members,
        })
    }

    pub fn required(&self) -> usize {
        self.required
    }

    pub fn members(&self) -> &[Member] {
        &self.members
    }
}
