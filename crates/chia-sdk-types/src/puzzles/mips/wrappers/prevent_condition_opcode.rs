use std::borrow::Cow;

use chia_puzzles::{PREVENT_CONDITION_OPCODE, PREVENT_CONDITION_OPCODE_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

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
        Cow::Borrowed(&PREVENT_CONDITION_OPCODE)
    }

    fn mod_hash() -> TreeHash {
        PREVENT_CONDITION_OPCODE_HASH.into()
    }
}
