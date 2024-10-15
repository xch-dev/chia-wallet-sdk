use chia_sdk_types::Conditions;

use crate::{DriverError, Spend, SpendContext};

pub trait SpendWithConditions {
    fn spend_with_conditions(
        &self,
        ctx: &mut SpendContext,
        conditions: Conditions,
    ) -> Result<Spend, DriverError>;
}
