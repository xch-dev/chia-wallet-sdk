use std::borrow::Cow;

use chia_puzzles::{FORCE_COIN_MESSAGE, FORCE_COIN_MESSAGE_HASH};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForceCoinMessageMod;

impl Mod for ForceCoinMessageMod {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&FORCE_COIN_MESSAGE)
    }

    fn mod_hash() -> TreeHash {
        FORCE_COIN_MESSAGE_HASH.into()
    }
}
