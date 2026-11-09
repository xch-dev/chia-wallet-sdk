use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{SECP256R1_MEMBER_PUZZLE_ASSERT, SECP256R1_MEMBER_PUZZLE_ASSERT_HASH};
use chia_secp::{R1PublicKey, R1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS secp256r1 member puzzle that signs with the puzzle hash instead
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
pub struct R1MemberPuzzleAssert {
    /// The secp256r1 public key that must sign.
    pub public_key: R1PublicKey,
}

impl R1MemberPuzzleAssert {
    /// Creates a new [`R1MemberPuzzleAssert`] from the public key.
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

/// The member solution for [`R1MemberPuzzleAssert`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct R1MemberPuzzleAssertSolution {
    /// The puzzle hash of the coin being spent.
    pub puzzle_hash: Bytes32,
    /// The signature on the hash of the delegated puzzle hash and puzzle hash.
    pub signature: R1Signature,
}

impl R1MemberPuzzleAssertSolution {
    /// Creates a new [`R1MemberPuzzleAssertSolution`] from the puzzle hash and signature.
    pub fn new(puzzle_hash: Bytes32, signature: R1Signature) -> Self {
        Self {
            puzzle_hash,
            signature,
        }
    }
}
