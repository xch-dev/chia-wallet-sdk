use chia_sdk_types::Condition;
use clvm_traits::{FromClvm, match_quote};
use clvm_utils::tree_hash;
use clvmr::Allocator;

use crate::{DriverError, HashedPtr, Spend};

pub fn parse_delegated_spend(
    allocator: &Allocator,
    delegated_spend: Spend,
) -> Result<Vec<Condition>, DriverError> {
    if tree_hash(allocator, delegated_spend.solution) != HashedPtr::NIL.tree_hash() {
        return Err(DriverError::InvalidDelegatedSpendFormat);
    }

    let (_, conditions) =
        <match_quote!(Vec<Condition>)>::from_clvm(allocator, delegated_spend.puzzle)?;

    Ok(conditions)
}
