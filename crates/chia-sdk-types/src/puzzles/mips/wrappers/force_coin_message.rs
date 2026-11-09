use std::borrow::Cow;

use chia_puzzles::{FORCE_COIN_MESSAGE, FORCE_COIN_MESSAGE_HASH};
use clvm_utils::TreeHash;

use crate::Mod;

/// The MIPS `force_coin_message` restriction, which requires the conditions to contain a
/// `SEND_MESSAGE` whose receiver is identified by its coin id.
///
/// It checks that the lowest three bits of the mode are set. This makes the spend
/// non-replayable, since it can only happen alongside the spend of the receiving coin. It has no
/// curried arguments.
///
/// Like the other restrictions that take a list of conditions, it can be used as a member
/// validator in [`RestrictionsArgs`](crate::puzzles::RestrictionsArgs) or as a delegated puzzle
/// wrapper with [`AddDelegatedPuzzleWrapper`](crate::puzzles::AddDelegatedPuzzleWrapper).
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
