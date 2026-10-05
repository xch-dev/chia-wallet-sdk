use chia_sdk_types::puzzles::{
    EnforceDelegatedPuzzleWrappers, Force1of2RestrictedVariable, Timelock,
};

use super::{RestrictionMemo, WrapperMemo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedRestriction {
    /// Includes the restrictions on the new right side, which can be parsed recursively.
    Force1of2RestrictedVariable(Force1of2RestrictedVariable, Vec<RestrictionMemo>),
    EnforceDelegatedPuzzleWrappers(EnforceDelegatedPuzzleWrappers, Vec<WrapperMemo>),
    Timelock(Timelock),
}
