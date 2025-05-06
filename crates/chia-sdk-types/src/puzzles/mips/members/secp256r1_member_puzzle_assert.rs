use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_secp::{R1PublicKey, R1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct Secp256r1MemberPuzzleAssert {
    pub public_key: R1PublicKey,
}

impl Secp256r1MemberPuzzleAssert {
    pub fn new(public_key: R1PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for Secp256r1MemberPuzzleAssert {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SECP256R1_MEMBER_PUZZLE_ASSERT)
    }

    fn mod_hash() -> TreeHash {
        SECP256R1_MEMBER_PUZZLE_ASSERT_HASH
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct Secp256r1MemberPuzzleAssertSolution {
    pub puzzle_hash: Bytes32,
    pub signature: R1Signature,
}

impl Secp256r1MemberPuzzleAssertSolution {
    pub fn new(puzzle_hash: Bytes32, signature: R1Signature) -> Self {
        Self {
            puzzle_hash,
            signature,
        }
    }
}

pub const SECP256R1_MEMBER_PUZZLE_ASSERT: [u8; 53] = hex!(
    "
    ff02ffff01ff04ffff04ff02ffff04ff17ff808080ffff841c3a8f00ff05ffff
    0bff0bff1780ff2f8080ffff04ffff0148ff018080
    "
);

pub const SECP256R1_MEMBER_PUZZLE_ASSERT_HASH: TreeHash = TreeHash::new(hex!(
    "d77bbc050bff8dfe4eb4544fa2bf0d0fd0463b96801bf6445687bd35985e71db"
));
