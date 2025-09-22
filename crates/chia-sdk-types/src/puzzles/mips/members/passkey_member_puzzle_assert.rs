use std::borrow::Cow;

use chia_protocol::{Bytes, Bytes32};
use chia_puzzles::{PASSKEY_MEMBER_PUZZLE_ASSERT, PASSKEY_MEMBER_PUZZLE_ASSERT_HASH};
use chia_secp::{R1PublicKey, R1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct PasskeyMemberPuzzleAssert {
    pub public_key: R1PublicKey,
}

impl PasskeyMemberPuzzleAssert {
    pub fn new(public_key: R1PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for PasskeyMemberPuzzleAssert {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&PASSKEY_MEMBER_PUZZLE_ASSERT)
    }

    fn mod_hash() -> TreeHash {
        PASSKEY_MEMBER_PUZZLE_ASSERT_HASH.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct PasskeyMemberPuzzleAssertSolution {
    pub authenticator_data: Bytes,
    pub client_data_json: Bytes,
    pub challenge_index: usize,
    pub signature: R1Signature,
    pub puzzle_hash: Bytes32,
}
