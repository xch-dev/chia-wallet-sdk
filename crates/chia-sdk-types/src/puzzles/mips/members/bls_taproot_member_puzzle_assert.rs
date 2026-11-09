use std::borrow::Cow;

use chia_bls::PublicKey;
use chia_puzzles::{
    BLS_WITH_TAPROOT_MEMBER_PUZZLE_ASSERT, BLS_WITH_TAPROOT_MEMBER_PUZZLE_ASSERT_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS BLS taproot member puzzle that signs with the puzzle hash
/// instead of the coin id.
///
/// If the solution doesn't include an original public key, it outputs an `AGG_SIG_PUZZLE` condition
/// that requires `synthetic_key` to sign the delegated puzzle hash. Otherwise, the delegated puzzle
/// is treated as the hidden puzzle, and it asserts that `synthetic_key` equals the original public
/// key plus the key derived from the hash of the original public key and the delegated puzzle hash.
/// In that case, no signature is required. Signatures can be reused for any coin with the same
/// puzzle hash.
///
/// Like all MIPS members, it receives the delegated puzzle hash as the first value of its solution
/// from the layer above, followed by the member's own solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BlsTaprootMemberPuzzleAssert {
    /// The synthetic public key, which is derived from the original public key and the hidden
    /// puzzle hash.
    pub synthetic_key: PublicKey,
}

impl BlsTaprootMemberPuzzleAssert {
    /// Creates a new [`BlsTaprootMemberPuzzleAssert`] from the synthetic public key.
    pub fn new(synthetic_key: PublicKey) -> Self {
        Self { synthetic_key }
    }
}

impl Mod for BlsTaprootMemberPuzzleAssert {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&BLS_WITH_TAPROOT_MEMBER_PUZZLE_ASSERT)
    }

    fn mod_hash() -> TreeHash {
        BLS_WITH_TAPROOT_MEMBER_PUZZLE_ASSERT_HASH.into()
    }
}

/// The member solution for the MIPS BLS taproot member puzzle that signs with the puzzle hash.
/// See [`BlsTaprootMemberPuzzleAssert`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BlsTaprootMemberPuzzleAssertSolution {
    /// The original public key, if the delegated puzzle is the hidden puzzle. Use [`None`] to
    /// sign with the synthetic key instead.
    pub original_public_key: Option<PublicKey>,
}

impl BlsTaprootMemberPuzzleAssertSolution {
    /// Creates a new [`BlsTaprootMemberPuzzleAssertSolution`] from the optional original public
    /// key.
    pub fn new(original_public_key: Option<PublicKey>) -> Self {
        Self {
            original_public_key,
        }
    }
}
