use chia_sdk_types::puzzles::{
    EnforceDelegatedPuzzleWrappers, Force1of2RestrictedVariable, Timelock,
};

use super::WrapperMemo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedRestriction {
    Force1of2RestrictedVariable(Force1of2RestrictedVariable),
    EnforceDelegatedPuzzleWrappers(EnforceDelegatedPuzzleWrappers, Vec<WrapperMemo>),
    Timelock(Timelock),
}
