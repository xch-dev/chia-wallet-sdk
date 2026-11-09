use chia_sdk_types::puzzles::{
    EnforceDelegatedPuzzleWrappers, Force1of2RestrictedVariable, Timelock,
};

use super::{RestrictionMemo, WrapperMemo};

/// A MIPS restriction identified by [`RestrictionMemo::parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedRestriction {
    /// A member condition validator that forces created coins into a 1 of 2 with a fixed left side.
    /// Includes the restrictions on the new right side, which can be parsed recursively.
    Force1of2RestrictedVariable(Force1of2RestrictedVariable, Vec<RestrictionMemo>),
    /// A delegated puzzle validator that enforces a stack of delegated puzzle wrappers, along
    /// with the memos of those wrappers, which can be parsed with [`WrapperMemo::parse`].
    EnforceDelegatedPuzzleWrappers(EnforceDelegatedPuzzleWrappers, Vec<WrapperMemo>),
    /// A member condition validator that requires an `ASSERT_SECONDS_RELATIVE` condition with
    /// exactly the given number of seconds.
    Timelock(Timelock),
}
