use std::collections::HashMap;

use chia_sdk_types::Conditions;
use clvm_traits::clvm_quote;
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{DriverError, Spend, SpendContext};

use super::MemberSpend;

#[derive(Debug, Clone)]
pub struct VaultSpend {
    pub delegated: Spend,
    pub members: HashMap<TreeHash, MemberSpend>,
    pub restrictions: HashMap<TreeHash, Spend>,
}

impl VaultSpend {
    pub fn with_conditions(
        ctx: &mut SpendContext,
        conditions: Conditions,
    ) -> Result<Self, DriverError> {
        let delegated = Spend::new(ctx.alloc(&clvm_quote!(conditions))?, NodePtr::NIL);
        Ok(Self::new(delegated))
    }

    pub fn new(delegated_spend: Spend) -> Self {
        Self {
            delegated: delegated_spend,
            members: HashMap::new(),
            restrictions: HashMap::new(),
        }
    }
}
