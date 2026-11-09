use chia_sdk_types::puzzles::{Force1of2RestrictedVariable, PreventConditionOpcode, Timelock};

use super::RestrictionMemo;

/// A delegated puzzle wrapper identified by [`WrapperMemo::parse`](crate::WrapperMemo::parse).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedWrapper {
    /// Requires the delegated puzzle to assert a coin announcement.
    ForceAssertCoinAnnouncement,
    /// Requires the delegated puzzle to send a message to a receiver identified by its coin id.
    ForceCoinMessage,
    /// Requires the singleton to be recreated with the same amount, so it can't be melted.
    ForceSingletonRecreation,
    /// Prevents the delegated puzzle from outputting conditions with the given opcode.
    PreventConditionOpcode(PreventConditionOpcode),
    /// Requires the delegated puzzle to create exactly one coin.
    PreventMultipleCreateCoins,
    /// Requires an `ASSERT_SECONDS_RELATIVE` condition with exactly the given number of seconds.
    Timelock(Timelock),
    /// Includes the restrictions on the new right side, which can be parsed recursively.
    Force1of2RestrictedVariable(Force1of2RestrictedVariable, Vec<RestrictionMemo>),
}
