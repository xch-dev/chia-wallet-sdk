use chia_sdk_types::Conditions;
use clvm_utils::TreeHash;

use crate::{DriverError, Spend, SpendContext};

pub trait SpendWithConditions {
    fn puzzle_hash(&self) -> TreeHash;

    fn spend_with_conditions(
        &self,
        ctx: &mut SpendContext,
        conditions: Conditions,
    ) -> Result<Spend, DriverError>;
}
