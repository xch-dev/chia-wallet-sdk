use chia_sdk_types::puzzles::{Force1of2RestrictedVariable, PreventConditionOpcode, Timelock};

use super::RestrictionMemo;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedWrapper {
    ForceAssertCoinAnnouncement,
    ForceCoinMessage,
    ForceSingletonRecreation,
    PreventConditionOpcode(PreventConditionOpcode),
    PreventMultipleCreateCoins,
    Timelock(Timelock),
    /// Includes the restrictions on the new right side, which can be parsed recursively.
    Force1of2RestrictedVariable(Force1of2RestrictedVariable, Vec<RestrictionMemo>),
}
