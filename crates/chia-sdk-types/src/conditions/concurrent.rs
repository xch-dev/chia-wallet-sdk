use chia_protocol::Bytes32;
use clvm_traits::{apply_constants, FromClvm, ToClvm};

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertConcurrentSpend {
    #[clvm(constant = 64)]
    pub opcode: u8,
    pub coin_id: Bytes32,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertConcurrentPuzzle {
    #[clvm(constant = 65)]
    pub opcode: u8,
    pub puzzle_hash: Bytes32,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertEphemeral {
    #[clvm(constant = 76)]
    pub opcode: u8,
}
