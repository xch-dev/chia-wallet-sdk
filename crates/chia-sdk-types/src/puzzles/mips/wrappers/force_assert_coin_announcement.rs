use std::borrow::Cow;

use chia_puzzles::{FORCE_ASSERT_COIN_ANNOUNCEMENT, FORCE_ASSERT_COIN_ANNOUNCEMENT_HASH};
use clvm_utils::TreeHash;

use crate::Mod;

/// The MIPS `force_assert_coin_announcement` restriction, which requires the conditions to
/// contain an `ASSERT_COIN_ANNOUNCEMENT`.
///
/// This makes the spend non-replayable, since it can only happen alongside the spend of the coin
/// that made the announcement. It has no curried arguments.
///
/// Like the other restrictions that take a list of conditions, it can be used as a member
/// validator in [`RestrictionsArgs`](crate::puzzles::RestrictionsArgs) or as a delegated puzzle
/// wrapper with [`AddDelegatedPuzzleWrapper`](crate::puzzles::AddDelegatedPuzzleWrapper).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForceAssertCoinAnnouncementMod;

impl Mod for ForceAssertCoinAnnouncementMod {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&FORCE_ASSERT_COIN_ANNOUNCEMENT)
    }

    fn mod_hash() -> TreeHash {
        FORCE_ASSERT_COIN_ANNOUNCEMENT_HASH.into()
    }
}
