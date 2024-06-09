use chia_protocol::Bytes32;
use clvm_traits::{apply_constants, FromClvm, ToClvm};

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertMyCoinId {
    #[clvm(constant = 70)]
    pub opcode: u8,
    pub coin_id: Bytes32,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertMyParentId {
    #[clvm(constant = 71)]
    pub opcode: u8,
    pub parent_id: Bytes32,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertMyPuzzleHash {
    #[clvm(constant = 72)]
    pub opcode: u8,
    pub puzzle_hash: Bytes32,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertMyAmount {
    #[clvm(constant = 73)]
    pub opcode: u8,
    pub amount: u64,
}

impl AssertMyAmount {
    pub fn new(amount: u64) -> Self {
        Self { amount }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertMyBirthSeconds {
    #[clvm(constant = 74)]
    pub opcode: u8,
    pub seconds: u64,
}

impl AssertMyBirthSeconds {
    pub fn new(seconds: u64) -> Self {
        Self { seconds }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertMyBirthHeight {
    #[clvm(constant = 75)]
    pub opcode: u8,
    pub height: u32,
}

impl AssertMyBirthHeight {
    pub fn new(height: u32) -> Self {
        Self { height }
    }
}
