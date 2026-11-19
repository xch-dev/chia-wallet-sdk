use chia_consensus::{flags::ConsensusFlags, opcodes::parse_opcode};
use chia_sdk_types::Condition;
use clvm_traits::{FromClvm, match_quote};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Spend};

/// Extracts the conditions of a delegated spend whose puzzle is a quoted list of conditions, or returns
/// [`DriverError::InvalidDelegatedSpendFormat`]. The solution is ignored.
///
/// A delegated spend can technically be any puzzle and solution. The puzzle is the only
/// thing that gets signed, so the solution allows malleability in behavior if needed.
/// However, for the purposes of clear signing, we don't *want* malleability. We want the
/// conditions that are output to be static. So in lieu of needing this flexibility in the
/// future, we extract out the standard quoted conditions format rather than running the
/// delegated spend to get its output. This ensures that the conditions are a fixed list.
///
/// A condition with a known consensus opcode that doesn't parse as its [`Condition`] variant is also
/// [`DriverError::InvalidDelegatedSpendFormat`], since it would otherwise be ignored.
pub fn parse_delegated_spend(
    allocator: &Allocator,
    delegated_spend: Spend,
) -> Result<Vec<Condition>, DriverError> {
    let (_, conditions) = <match_quote!(NodePtr)>::from_clvm(allocator, delegated_spend.puzzle)
        .map_err(|_| DriverError::InvalidDelegatedSpendFormat)?;

    parse_conditions(allocator, conditions).ok_or(DriverError::InvalidDelegatedSpendFormat)
}

/// Parses a list of conditions for clear signing, or returns [`None`] if any of them would be
/// misunderstood.
///
/// A condition that doesn't match any other variant is kept as [`Condition::Other`] and is otherwise
/// ignored, which is only safe if consensus ignores it too. So a condition with a known consensus
/// opcode must parse as its variant. This rejects anything that only looks like a known condition,
/// such as a negative `CREATE_COIN` amount other than the canonical `-113`, or a `-113` output whose
/// puzzle hash isn't nil or 32 bytes (which only the singleton and CAT puzzles understand). Consensus
/// would reject most of these anyways, and refusing the rest is always safe.
///
/// Two byte opcodes (and one byte opcodes that consensus doesn't know) are unknown conditions, which
/// consensus ignores apart from their cost.
pub(crate) fn parse_conditions(
    allocator: &Allocator,
    conditions: NodePtr,
) -> Option<Vec<Condition>> {
    let conditions = Vec::<Condition>::from_clvm(allocator, conditions).ok()?;

    for condition in &conditions {
        if let Condition::Other(condition) = condition {
            let (opcode, _) = <(NodePtr, NodePtr)>::from_clvm(allocator, *condition).ok()?;

            if parse_opcode(allocator, opcode, ConsensusFlags::empty())
                .is_some_and(|opcode| u8::try_from(opcode).is_ok())
            {
                return None;
            }
        }
    }

    Some(conditions)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_sdk_test::BlsPair;
    use chia_sdk_types::Conditions;

    use crate::{SpendContext, SpendWithConditions, StandardLayer};

    use super::*;

    #[test]
    fn test_clear_signing_delegated_spend() -> Result<()> {
        let mut ctx = SpendContext::new();

        let pair = BlsPair::new(1337);
        let spend =
            StandardLayer::new(pair.pk).spend_with_conditions(&mut ctx, Conditions::new())?;

        let result = parse_delegated_spend(&ctx, spend);

        assert!(matches!(
            result,
            Err(DriverError::InvalidDelegatedSpendFormat)
        ));

        Ok(())
    }
}
