use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{FIXED_PUZZLE_MEMBER, FIXED_PUZZLE_MEMBER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS fixed puzzle member, which only approves one specific delegated
/// puzzle hash.
///
/// It fails unless the delegated puzzle hash equals `fixed_puzzle_hash`, and outputs no
/// conditions. It has no solution of its own.
///
/// Like all MIPS members, it receives the delegated puzzle hash as the first value of its solution
/// from the layer above, followed by the member's own solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct FixedPuzzleMember {
    /// The only delegated puzzle hash that this member approves.
    pub fixed_puzzle_hash: Bytes32,
}

impl FixedPuzzleMember {
    /// Creates a new [`FixedPuzzleMember`] from the delegated puzzle hash it approves.
    pub fn new(fixed_puzzle_hash: Bytes32) -> Self {
        Self { fixed_puzzle_hash }
    }
}

impl Mod for FixedPuzzleMember {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&FIXED_PUZZLE_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        FIXED_PUZZLE_MEMBER_HASH.into()
    }
}
