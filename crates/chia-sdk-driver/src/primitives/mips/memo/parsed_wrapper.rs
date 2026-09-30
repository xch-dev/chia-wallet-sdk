use chia_sdk_types::puzzles::{Force1of2RestrictedVariable, PreventConditionOpcode, Timelock};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedWrapper {
    ForceAssertCoinAnnouncement,
    ForceCoinMessage,
    ForceSingletonRecreation,
    PreventConditionOpcode(PreventConditionOpcode),
    PreventMultipleCreateCoins,
    Timelock(Timelock),
    Force1of2RestrictedVariable(Force1of2RestrictedVariable),
}
