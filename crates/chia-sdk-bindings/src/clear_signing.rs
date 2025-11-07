use chia_protocol::Bytes32;
use chia_sdk_driver::{self as sdk};
use clvm_utils::TreeHash;

use crate::Spend;

#[derive(Clone)]
pub struct VaultSpendReveal {
    pub launcher_id: Bytes32,
    pub custody_hash: TreeHash,
    pub delegated_spend: Spend,
}

impl From<VaultSpendReveal> for sdk::VaultSpendReveal {
    fn from(value: VaultSpendReveal) -> Self {
        Self {
            launcher_id: value.launcher_id,
            custody_hash: value.custody_hash,
            delegated_spend: value.delegated_spend.into(),
        }
    }
}
