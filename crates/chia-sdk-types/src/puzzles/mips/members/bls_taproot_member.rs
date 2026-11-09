use std::borrow::Cow;

use chia_bls::PublicKey;
use chia_puzzles::{BLS_WITH_TAPROOT_MEMBER, BLS_WITH_TAPROOT_MEMBER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS BLS taproot member puzzle, which approves a delegated puzzle
/// hash with either a signature or a hidden puzzle.
///
/// If the solution doesn't include an original public key, it outputs an `AGG_SIG_ME` condition
/// that requires `synthetic_key` to sign the delegated puzzle hash. Otherwise, the delegated puzzle
/// is treated as the hidden puzzle, and it asserts that `synthetic_key` equals the original public
/// key plus the key derived from the hash of the original public key and the delegated puzzle hash.
/// In that case, no signature is required.
///
/// Like all MIPS members, it receives the delegated puzzle hash as the first value of its solution
/// from the layer above, followed by the member's own solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BlsTaprootMember {
    /// The synthetic public key, which is derived from the original public key and the hidden
    /// puzzle hash.
    pub synthetic_key: PublicKey,
}

impl BlsTaprootMember {
    /// Creates a new [`BlsTaprootMember`] from the synthetic public key.
    pub fn new(synthetic_key: PublicKey) -> Self {
        Self { synthetic_key }
    }
}

impl Mod for BlsTaprootMember {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&BLS_WITH_TAPROOT_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        BLS_WITH_TAPROOT_MEMBER_HASH.into()
    }
}

/// The member solution for the MIPS BLS taproot member puzzle. See [`BlsTaprootMember`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BlsTaprootMemberSolution {
    /// The original public key, if the delegated puzzle is the hidden puzzle. Use [`None`] to
    /// sign with the synthetic key instead.
    pub original_public_key: Option<PublicKey>,
}

impl BlsTaprootMemberSolution {
    /// Creates a new [`BlsTaprootMemberSolution`] from the optional original public key.
    pub fn new(original_public_key: Option<PublicKey>) -> Self {
        Self {
            original_public_key,
        }
    }
}
