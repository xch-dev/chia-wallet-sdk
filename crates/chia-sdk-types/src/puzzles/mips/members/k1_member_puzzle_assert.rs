use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{SECP256K1_MEMBER_PUZZLE_ASSERT, SECP256K1_MEMBER_PUZZLE_ASSERT_HASH};
use chia_secp::{K1PublicKey, K1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct K1MemberPuzzleAssert {
    pub public_key: K1PublicKey,
}

impl K1MemberPuzzleAssert {
    pub fn new(public_key: K1PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for K1MemberPuzzleAssert {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SECP256K1_MEMBER_PUZZLE_ASSERT)
    }

    fn mod_hash() -> TreeHash {
        SECP256K1_MEMBER_PUZZLE_ASSERT_HASH.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct K1MemberPuzzleAssertSolution {
    pub puzzle_hash: Bytes32,
    pub signature: K1Signature,
}

impl K1MemberPuzzleAssertSolution {
    pub fn new(puzzle_hash: Bytes32, signature: K1Signature) -> Self {
        Self {
            puzzle_hash,
            signature,
        }
    }
}
