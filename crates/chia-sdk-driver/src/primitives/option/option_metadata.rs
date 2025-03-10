use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct OptionMetadata {
    pub expiration_seconds: u64,
    pub underlying_type: OptionType,
    pub strike_type: OptionType,
}

impl OptionMetadata {
    pub fn new(
        expiration_seconds: u64,
        underlying_type: OptionType,
        strike_type: OptionType,
    ) -> Self {
        Self {
            expiration_seconds,
            underlying_type,
            strike_type,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
#[repr(u8)]
pub enum OptionType {
    Xch,
    Cat(Bytes32),
}
