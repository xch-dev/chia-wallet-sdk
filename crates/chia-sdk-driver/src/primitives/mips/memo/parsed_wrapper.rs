use chia_sdk_types::puzzles::{PreventConditionOpcode, Timelock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedWrapper {
    ForceAssertCoinAnnouncement,
    ForceCoinMessage,
    PreventConditionOpcode(PreventConditionOpcode),
    PreventMultipleCreateCoins,
    Timelock(Timelock),
}
