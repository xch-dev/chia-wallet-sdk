use clvm_traits::{apply_constants, FromClvm, ToClvm};

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertSecondsRelative {
    #[clvm(constant = 80)]
    pub opcode: u8,
    pub seconds: u64,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertSecondsAbsolute {
    #[clvm(constant = 81)]
    pub opcode: u8,
    pub seconds: u64,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertHeightRelative {
    #[clvm(constant = 82)]
    pub opcode: u8,
    pub height: u32,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertHeightAbsolute {
    #[clvm(constant = 83)]
    pub opcode: u8,
    pub height: u32,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertBeforeSecondsRelative {
    #[clvm(constant = 84)]
    pub opcode: u8,
    pub seconds: u64,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertBeforeSecondsAbsolute {
    #[clvm(constant = 85)]
    pub opcode: u8,
    pub seconds: u64,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertBeforeHeightRelative {
    #[clvm(constant = 86)]
    pub opcode: u8,
    pub height: u32,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct AssertBeforeHeightAbsolute {
    #[clvm(constant = 87)]
    pub opcode: u8,
    pub height: u32,
}
