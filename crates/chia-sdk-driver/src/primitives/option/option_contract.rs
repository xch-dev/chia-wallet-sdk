use crate::{DriverError, SpendContext};

use super::{OptionMetadata, OptionSingleton};

#[derive(Debug, Clone, Copy)]
pub struct OptionContract {
    pub singleton: OptionSingleton,
    pub metadata: OptionMetadata,
}

impl OptionContract {
    pub fn exercise(&self, ctx: &mut SpendContext) -> Result<(), DriverError> {
        Ok(())
    }
}
