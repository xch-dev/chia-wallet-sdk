use std::borrow::Cow;

use chia_puzzles::{FORCE_ASSERT_COIN_ANNOUNCEMENT, FORCE_ASSERT_COIN_ANNOUNCEMENT_HASH};
use clvm_utils::TreeHash;

use crate::Mod;

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
