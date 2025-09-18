use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{FIXED_PUZZLE_MEMBER, FIXED_PUZZLE_MEMBER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct FixedPuzzleMember {
    pub fixed_puzzle_hash: Bytes32,
}

impl FixedPuzzleMember {
    pub fn new(fixed_puzzle_hash: Bytes32) -> Self {
        Self { fixed_puzzle_hash }
    }
}

impl Mod for FixedPuzzleMember {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&FIXED_PUZZLE_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        FIXED_PUZZLE_MEMBER_HASH.into()
    }
}
