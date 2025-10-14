use std::borrow::Cow;

use chia_bls::PublicKey;
use chia_puzzles::{BLS_MEMBER_PUZZLE_ASSERT, BLS_MEMBER_PUZZLE_ASSERT_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BlsMemberPuzzleAssert {
    pub public_key: PublicKey,
}

impl BlsMemberPuzzleAssert {
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for BlsMemberPuzzleAssert {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&BLS_MEMBER_PUZZLE_ASSERT)
    }

    fn mod_hash() -> TreeHash {
        BLS_MEMBER_PUZZLE_ASSERT_HASH.into()
    }
}
