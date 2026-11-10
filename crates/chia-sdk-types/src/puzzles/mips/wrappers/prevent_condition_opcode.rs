use std::borrow::Cow;

use chia_puzzles::{PREVENT_CONDITION_OPCODE, PREVENT_CONDITION_OPCODE_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS `prevent_condition_opcode` restriction, which fails if any of
/// the conditions has the given opcode.
///
/// Like the other restrictions that take a list of conditions, it can be used as a member
/// validator in [`RestrictionsArgs`](crate::puzzles::RestrictionsArgs) or as a delegated puzzle
/// wrapper with [`AddDelegatedPuzzleWrapper`](crate::puzzles::AddDelegatedPuzzleWrapper).
///
/// ## Example
///
/// ```rust
/// # use chia_sdk_types::{Mod, puzzles::PreventConditionOpcode};
/// // Prevent coin announcements (opcode 60) and puzzle announcements (opcode 62).
/// let coin = PreventConditionOpcode::new(60).curry_tree_hash();
/// let puzzle = PreventConditionOpcode::new(62).curry_tree_hash();
/// assert_ne!(coin, puzzle);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct PreventConditionOpcode {
    /// The condition opcode that isn't allowed.
    pub condition_opcode: u16,
}

impl PreventConditionOpcode {
    /// Creates a new [`PreventConditionOpcode`] restriction for the given opcode.
    pub fn new(condition_opcode: u16) -> Self {
        Self { condition_opcode }
    }
}

impl Mod for PreventConditionOpcode {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&PREVENT_CONDITION_OPCODE)
    }

    fn mod_hash() -> TreeHash {
        PREVENT_CONDITION_OPCODE_HASH.into()
    }
}
