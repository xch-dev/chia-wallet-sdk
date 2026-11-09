use std::borrow::Cow;

use chia_bls::PublicKey;
use chia_puzzles::{BLS_MEMBER_PUZZLE_ASSERT, BLS_MEMBER_PUZZLE_ASSERT_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS BLS member puzzle that signs with the puzzle hash instead of the
/// coin id.
///
/// It outputs an `AGG_SIG_PUZZLE` condition that requires `public_key` to sign the delegated
/// puzzle hash. Unlike [`BlsMember`](super::BlsMember), the signature can be reused for any coin
/// with the same puzzle hash, so be aware of replays. It has no solution of its own.
///
/// Like all MIPS members, it receives the delegated puzzle hash as the first value of its solution
/// from the layer above, followed by the member's own solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BlsMemberPuzzleAssert {
    /// The public key that must sign the delegated puzzle hash.
    pub public_key: PublicKey,
}

impl BlsMemberPuzzleAssert {
    /// Creates a new [`BlsMemberPuzzleAssert`] from the public key.
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for BlsMemberPuzzleAssert {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&BLS_MEMBER_PUZZLE_ASSERT)
    }

    fn mod_hash() -> TreeHash {
        BLS_MEMBER_PUZZLE_ASSERT_HASH.into()
    }
}
