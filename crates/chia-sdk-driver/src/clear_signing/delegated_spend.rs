use chia_sdk_types::Condition;
use clvm_traits::{FromClvm, match_quote};
use clvmr::Allocator;

use crate::{DriverError, Spend};

/// A delegated spend can technically be any puzzle and solution. The puzzle is the only
/// thing that gets signed, so the solution allows malleability in behavior if needed.
/// However, for the purposes of clear signing, we don't *want* malleability. We want the
/// conditions that are output to be static. So in lieu of needing this flexibility in the
/// future, we extract out the standard quoted conditions format rather than running the
/// delegated spend to get its output. This ensures that the conditions are a fixed list.
pub fn parse_delegated_spend(
    allocator: &Allocator,
    delegated_spend: Spend,
) -> Result<Vec<Condition>, DriverError> {
    let (_, conditions) =
        <match_quote!(Vec<Condition>)>::from_clvm(allocator, delegated_spend.puzzle)
            .map_err(|_| DriverError::InvalidDelegatedSpendFormat)?;

    Ok(conditions)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_protocol::Bytes;
    use chia_sdk_test::BlsPair;
    use chia_sdk_types::Conditions;
    use clvm_traits::clvm_quote;
    use clvmr::NodePtr;

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

    #[test]
    fn test_clear_signing_delegated_spend_malformed_condition() -> Result<()> {
        let mut ctx = SpendContext::new();

        // A SEND_MESSAGE condition whose mode isn't an atom can't be parsed, so it must not be ignored.
        let puzzle = ctx.alloc(&clvm_quote!([(66, ((1, 2), (Bytes::default(), ())))]))?;
        let spend = Spend::new(puzzle, NodePtr::NIL);

        let result = parse_delegated_spend(&ctx, spend);

        assert!(matches!(
            result,
            Err(DriverError::InvalidDelegatedSpendFormat)
        ));

        Ok(())
    }
}
