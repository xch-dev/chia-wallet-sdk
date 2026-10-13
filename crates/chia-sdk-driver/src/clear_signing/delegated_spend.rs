use chia_consensus::opcodes::CREATE_COIN;
use chia_sdk_types::Condition;
use clvm_traits::{FromClvm, match_quote};
use clvmr::{Allocator, NodePtr, SExp};
use num_bigint::BigInt;

use crate::{DriverError, Spend};

/// The `CREATE_COIN` amount which the singleton layer interprets as melting the singleton.
const MELT_SINGLETON_AMOUNT: i64 = -113;

/// A delegated spend can technically be any puzzle and solution. The puzzle is the only
/// thing that gets signed, so the solution allows malleability in behavior if needed.
/// However, for the purposes of clear signing, we don't *want* malleability. We want the
/// conditions that are output to be static. So in lieu of needing this flexibility in the
/// future, we extract out the standard quoted conditions format rather than running the
/// delegated spend to get its output. This ensures that the conditions are a fixed list.
///
/// Singleton melts are rejected, since they would otherwise be indistinguishable from a regular
/// `CREATE_COIN` condition. Use [`parse_delegated_spend_with_melt`] if melts should be allowed.
pub fn parse_delegated_spend(
    allocator: &Allocator,
    delegated_spend: Spend,
) -> Result<Vec<Condition>, DriverError> {
    let (conditions, melted) = parse_delegated_spend_with_melt(allocator, delegated_spend)?;

    if melted {
        return Err(DriverError::UnsupportedSingletonMelt);
    }

    Ok(conditions)
}

/// The same as [`parse_delegated_spend`], except that a `CREATE_COIN` condition with the singleton
/// melt amount is removed from the list of conditions, and the returned flag is set instead.
///
/// Any other `CREATE_COIN` condition with a negative amount is rejected.
pub fn parse_delegated_spend_with_melt(
    allocator: &Allocator,
    delegated_spend: Spend,
) -> Result<(Vec<Condition>, bool), DriverError> {
    let (_, raw_conditions) =
        <match_quote!(Vec<NodePtr>)>::from_clvm(allocator, delegated_spend.puzzle)
            .map_err(|_| DriverError::InvalidDelegatedSpendFormat)?;

    let mut conditions = Vec::with_capacity(raw_conditions.len());
    let mut melted = false;

    for raw_condition in raw_conditions {
        let condition = Condition::from_clvm(allocator, raw_condition)
            .map_err(|_| DriverError::InvalidDelegatedSpendFormat)?;

        // This is checked against the raw condition, since a negative amount can be decoded as a
        // number of different condition types depending on the other arguments.
        if let Some(amount) = create_coin_signed_amount(allocator, raw_condition)
            && amount < BigInt::ZERO
        {
            if amount != BigInt::from(MELT_SINGLETON_AMOUNT) {
                return Err(DriverError::InvalidDelegatedSpendFormat);
            }

            // Running a CAT TAIL uses the same amount, and is handled by the CAT parsing logic.
            if !matches!(condition, Condition::RunCatTail(_)) {
                if melted {
                    return Err(DriverError::InvalidDelegatedSpendFormat);
                }

                melted = true;
                continue;
            }
        }

        conditions.push(condition);
    }

    Ok((conditions, melted))
}

/// Decodes the amount of a `CREATE_COIN` condition as a signed integer, since the parsed condition
/// stores it as an unsigned integer. Returns `None` if the condition isn't a `CREATE_COIN`.
fn create_coin_signed_amount(allocator: &Allocator, condition: NodePtr) -> Option<BigInt> {
    let (opcode, (_puzzle_hash, (amount, _))) =
        <(NodePtr, (NodePtr, (NodePtr, NodePtr)))>::from_clvm(allocator, condition).ok()?;

    if !matches!(allocator.sexp(opcode), SExp::Atom)
        || allocator.number(opcode) != BigInt::from(CREATE_COIN)
    {
        return None;
    }

    match allocator.sexp(amount) {
        SExp::Atom => Some(allocator.number(amount)),
        SExp::Pair(..) => None,
    }
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
