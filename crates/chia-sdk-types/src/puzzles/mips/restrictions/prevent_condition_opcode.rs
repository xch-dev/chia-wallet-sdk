use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct PreventConditionOpcode {
    pub condition_opcode: u16,
}

impl PreventConditionOpcode {
    pub fn new(condition_opcode: u16) -> Self {
        Self { condition_opcode }
    }
}

impl Mod for PreventConditionOpcode {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&PREVENT_CONDITION_OPCODE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        PREVENT_CONDITION_OPCODE_PUZZLE_HASH
    }
}

pub const PREVENT_CONDITION_OPCODE_PUZZLE: [u8; 131] = hex!(
    "
    ff02ffff01ff02ffff03ffff02ff02ffff04ff02ffff04ff05ffff04ff0bff80
    80808080ffff010bffff01ff088080ff0180ffff04ffff01ff02ffff03ff0bff
    ff01ff02ffff03ffff09ff23ff0580ffff01ff0880ffff01ff02ff02ffff04ff
    02ffff04ff05ffff04ff1bff808080808080ff0180ffff01ff010180ff0180ff
    018080
    "
);

pub const PREVENT_CONDITION_OPCODE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "046dfa794bb1df14d5dc891b23764a0e31f119546d2c56cdc8df0d31daaa555f"
));
