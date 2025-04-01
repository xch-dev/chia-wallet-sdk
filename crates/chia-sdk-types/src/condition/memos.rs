use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm, ToClvmError};
use clvmr::{Allocator, NodePtr};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct Memos<T = NodePtr> {
    pub value: T,
}

impl<T> Memos<T> {
    pub fn new(value: T) -> Self {
        Self { value }
    }

    pub fn some(value: T) -> Option<Self> {
        Some(Self { value })
    }
}

impl Memos<NodePtr> {
    pub fn hint(allocator: &mut Allocator, hint: Bytes32) -> Result<Self, ToClvmError> {
        Ok(Self {
            value: [hint].to_clvm(allocator)?,
        })
    }
}
