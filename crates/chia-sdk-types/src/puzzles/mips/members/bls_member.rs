use std::borrow::Cow;

use chia_bls::PublicKey;
use chia_puzzles::{BLS_MEMBER, BLS_MEMBER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS BLS member puzzle, which approves a delegated puzzle hash with a
/// BLS signature.
///
/// It outputs an `AGG_SIG_ME` condition that requires `public_key` to sign the delegated puzzle
/// hash, so the signature is bound to the coin being spent. It has no solution of its own.
///
/// Like all MIPS members, it receives the delegated puzzle hash as the first value of its solution
/// from the layer above, followed by the member's own solution.
///
/// ## Example
///
/// ```rust
/// # use chia_bls::PublicKey;
/// # use chia_sdk_types::{Mod, puzzles::{BlsMember, BlsMemberPuzzleAssert}};
/// let public_key = PublicKey::default();
///
/// // The two variants have different puzzles, so they have different hashes.
/// assert_ne!(
///     BlsMember::new(public_key).curry_tree_hash(),
///     BlsMemberPuzzleAssert::new(public_key).curry_tree_hash()
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BlsMember {
    /// The public key that must sign the delegated puzzle hash.
    pub public_key: PublicKey,
}

impl BlsMember {
    /// Creates a new [`BlsMember`] from the public key.
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for BlsMember {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&BLS_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        BLS_MEMBER_HASH.into()
    }
}
