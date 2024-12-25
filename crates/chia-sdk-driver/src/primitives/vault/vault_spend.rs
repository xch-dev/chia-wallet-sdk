use std::collections::HashMap;

use clvm_utils::TreeHash;

use crate::Spend;

use super::MemberSpend;

#[derive(Debug, Clone)]
pub struct VaultSpend {
    pub delegated: Spend,
    pub members: HashMap<TreeHash, MemberSpend>,
    pub restrictions: HashMap<TreeHash, Spend>,
}

impl VaultSpend {
    pub fn new(delegated_spend: Spend) -> Self {
        Self {
            delegated: delegated_spend,
            members: HashMap::new(),
            restrictions: HashMap::new(),
        }
    }
}
