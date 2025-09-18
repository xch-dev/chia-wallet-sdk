use std::borrow::Cow;

use chia_puzzles::{TIMELOCK, TIMELOCK_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct Timelock {
    pub seconds: u64,
}

impl Timelock {
    pub fn new(seconds: u64) -> Self {
        Self { seconds }
    }
}

impl Mod for Timelock {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&TIMELOCK)
    }

    fn mod_hash() -> TreeHash {
        TIMELOCK_HASH.into()
    }
}
