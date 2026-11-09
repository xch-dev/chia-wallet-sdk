use std::borrow::Cow;

use chia_puzzles::{PREVENT_MULTIPLE_CREATE_COINS, PREVENT_MULTIPLE_CREATE_COINS_HASH};
use clvm_utils::TreeHash;

use crate::Mod;

/// The MIPS `prevent_multiple_create_coins` restriction, which requires the conditions to contain
/// exactly one `CREATE_COIN`.
///
/// Despite the name, it also fails if there are no `CREATE_COIN` conditions. It has no curried
/// arguments.
///
/// Like the other restrictions that take a list of conditions, it can be used as a member
/// validator in [`RestrictionsArgs`](crate::puzzles::RestrictionsArgs) or as a delegated puzzle
/// wrapper with [`AddDelegatedPuzzleWrapper`](crate::puzzles::AddDelegatedPuzzleWrapper).
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
