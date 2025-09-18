use std::borrow::Cow;

use chia_puzzles::{PREVENT_MULTIPLE_CREATE_COINS, PREVENT_MULTIPLE_CREATE_COINS_HASH};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreventMultipleCreateCoinsMod;

impl Mod for PreventMultipleCreateCoinsMod {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&PREVENT_MULTIPLE_CREATE_COINS)
    }

    fn mod_hash() -> TreeHash {
        PREVENT_MULTIPLE_CREATE_COINS_HASH.into()
    }
}
