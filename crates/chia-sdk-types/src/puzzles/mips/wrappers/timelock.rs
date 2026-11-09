use std::borrow::Cow;

use chia_puzzles::{TIMELOCK, TIMELOCK_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS `timelock` restriction, which requires the conditions to contain
/// an `ASSERT_SECONDS_RELATIVE` with exactly the given number of seconds.
///
/// Like the other restrictions that take a list of conditions, it can be used as a member
/// validator in [`RestrictionsArgs`](crate::puzzles::RestrictionsArgs) or as a delegated puzzle
/// wrapper with [`AddDelegatedPuzzleWrapper`](crate::puzzles::AddDelegatedPuzzleWrapper).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct Timelock {
    /// The number of seconds that must have passed since the coin was created.
    pub seconds: u64,
}

impl Timelock {
    /// Creates a new [`Timelock`] restriction for the given number of seconds.
    pub fn new(seconds: u64) -> Self {
        Self { seconds }
    }
}

impl Mod for Timelock {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&TIMELOCK)
    }

    fn mod_hash() -> TreeHash {
        TIMELOCK_HASH.into()
    }
}
