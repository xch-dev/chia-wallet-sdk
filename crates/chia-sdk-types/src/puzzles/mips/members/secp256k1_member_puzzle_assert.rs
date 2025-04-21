use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_secp::{K1PublicKey, K1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct Secp256k1MemberPuzzleAssert {
    pub public_key: K1PublicKey,
}

impl Secp256k1MemberPuzzleAssert {
    pub fn new(public_key: K1PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for Secp256k1MemberPuzzleAssert {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SECP256K1_MEMBER_PUZZLE_ASSERT)
    }

    fn mod_hash() -> TreeHash {
        SECP256K1_MEMBER_PUZZLE_ASSERT_HASH
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct Secp256k1MemberPuzzleAssertSolution {
    pub puzzle_hash: Bytes32,
    pub signature: K1Signature,
}

impl Secp256k1MemberPuzzleAssertSolution {
    pub fn new(puzzle_hash: Bytes32, signature: K1Signature) -> Self {
        Self {
            puzzle_hash,
            signature,
        }
    }
}

pub const SECP256K1_MEMBER_PUZZLE_ASSERT: [u8; 53] = hex!(
    "
    ff02ffff01ff04ffff04ff02ffff04ff17ff808080ffff8413d61f00ff05ffff
    0bff0bff1780ff2f8080ffff04ffff0148ff018080
    "
);

pub const SECP256K1_MEMBER_PUZZLE_ASSERT_HASH: TreeHash = TreeHash::new(hex!(
    "67d591ffeb00571269d401f41a6a43ceb927b5087074ad4446ff22400a010e87"
));
