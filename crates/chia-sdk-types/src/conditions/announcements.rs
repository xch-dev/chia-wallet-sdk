use chia_protocol::{Bytes, Bytes32};
use clvm_traits::{apply_constants, FromClvm, ToClvm};

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct CreateCoinAnnouncement {
    #[clvm(constant = 60)]
    pub opcode: u8,
    pub message: Bytes,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertCoinAnnouncement {
    #[clvm(constant = 61)]
    pub opcode: u8,
    pub announcement_id: Bytes32,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct CreatePuzzleAnnouncement {
    #[clvm(constant = 62)]
    pub opcode: u8,
    pub message: Bytes,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertPuzzleAnnouncement {
    #[clvm(constant = 63)]
    pub opcode: u8,
    pub announcement_id: Bytes32,
}
