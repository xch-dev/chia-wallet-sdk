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

impl AssertConcurrentSpend {
    pub fn new(coin_id: Bytes32) -> Self {
        Self { coin_id }
    }
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

impl AssertConcurrentPuzzle {
    pub fn new(puzzle_hash: Bytes32) -> Self {
        Self { puzzle_hash }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertEphemeral {
    #[clvm(constant = 76)]
    pub opcode: u8,
}

impl AssertEphemeral {
    pub fn new() -> Self {
        Self::default()
    }
}
