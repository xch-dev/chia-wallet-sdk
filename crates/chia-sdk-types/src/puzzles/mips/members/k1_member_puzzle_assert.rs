use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{SECP256K1_MEMBER_PUZZLE_ASSERT, SECP256K1_MEMBER_PUZZLE_ASSERT_HASH};
use chia_secp::{K1PublicKey, K1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS secp256k1 member puzzle that signs with the puzzle hash instead
/// of the coin id.
///
/// The signature must be on the SHA-256 hash of the delegated puzzle hash followed by the coin's
/// puzzle hash, and the puzzle outputs `ASSERT_MY_PUZZLE_HASH`. The signature can be reused for any
/// coin with the same puzzle hash, so be aware of replays.
///
/// Like all MIPS members, it receives the delegated puzzle hash as the first value of its solution
/// from the layer above, followed by the member's own solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct K1MemberPuzzleAssert {
    /// The secp256k1 public key that must sign.
    pub public_key: K1PublicKey,
}

impl K1MemberPuzzleAssert {
    /// Creates a new [`K1MemberPuzzleAssert`] from the public key.
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

/// The member solution for [`K1MemberPuzzleAssert`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct K1MemberPuzzleAssertSolution {
    /// The puzzle hash of the coin being spent.
    pub puzzle_hash: Bytes32,
    /// The signature on the hash of the delegated puzzle hash and puzzle hash.
    pub signature: K1Signature,
}

impl K1MemberPuzzleAssertSolution {
    /// Creates a new [`K1MemberPuzzleAssertSolution`] from the puzzle hash and signature.
    pub fn new(puzzle_hash: Bytes32, signature: K1Signature) -> Self {
        Self {
            puzzle_hash,
            signature,
        }
    }
}
