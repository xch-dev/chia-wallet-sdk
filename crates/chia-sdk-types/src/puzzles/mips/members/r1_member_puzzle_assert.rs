use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{SECP256R1_MEMBER_PUZZLE_ASSERT, SECP256R1_MEMBER_PUZZLE_ASSERT_HASH};
use chia_secp::{R1PublicKey, R1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct R1MemberPuzzleAssert {
    pub public_key: R1PublicKey,
}

impl R1MemberPuzzleAssert {
    pub fn new(public_key: R1PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for R1MemberPuzzleAssert {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SECP256R1_MEMBER_PUZZLE_ASSERT)
    }

    fn mod_hash() -> TreeHash {
        SECP256R1_MEMBER_PUZZLE_ASSERT_HASH.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct R1MemberPuzzleAssertSolution {
    pub puzzle_hash: Bytes32,
    pub signature: R1Signature,
}

impl R1MemberPuzzleAssertSolution {
    pub fn new(puzzle_hash: Bytes32, signature: R1Signature) -> Self {
        Self {
            puzzle_hash,
            signature,
        }
    }
}
