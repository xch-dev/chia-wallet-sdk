use std::collections::HashMap;

use clvm_utils::TreeHash;

use crate::{DriverError, Spend, SpendContext};

use super::InnerPuzzleSpend;

#[derive(Debug, Clone)]
pub struct MipsSpend {
    pub delegated: Spend,
    pub members: HashMap<TreeHash, InnerPuzzleSpend>,
    pub restrictions: HashMap<TreeHash, Spend>,
}

impl MipsSpend {
    pub fn new(delegated_spend: Spend) -> Self {
        Self {
            delegated: delegated_spend,
            members: HashMap::new(),
            restrictions: HashMap::new(),
        }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        custody_hash: TreeHash,
    ) -> Result<Spend, DriverError> {
        self.members
            .get(&custody_hash)
            .ok_or(DriverError::MissingSubpathSpend)?
            .spend(ctx, self, &mut Vec::new(), true)
    }
}
