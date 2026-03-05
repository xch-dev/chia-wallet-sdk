use bindy::Result;
use chia_protocol::{Bytes, Bytes32};
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

pub fn calculate_vault_puzzle_message(
    delegated_puzzle_hash: Bytes32,
    vault_puzzle_hash: Bytes32,
) -> Result<Bytes> {
    Ok(sdk::calculate_vault_puzzle_message(
        delegated_puzzle_hash,
        vault_puzzle_hash,
    ))
}

pub fn calculate_vault_coin_message(
    delegated_puzzle_hash: Bytes32,
    vault_coin_id: Bytes32,
    genesis_challenge: Bytes32,
) -> Result<Bytes> {
    Ok(sdk::calculate_vault_coin_message(
        delegated_puzzle_hash,
        vault_coin_id,
        genesis_challenge,
    ))
}

pub fn calculate_vault_start_recovery_message(
    delegated_puzzle_hash: Bytes32,
    left_side_subtree_hash: Bytes32,
    recovery_timelock: u64,
    vault_coin_id: Bytes32,
    genesis_challenge: Bytes32,
) -> Result<Bytes> {
    Ok(sdk::calculate_vault_start_recovery_message(
        delegated_puzzle_hash,
        left_side_subtree_hash,
        recovery_timelock,
        vault_coin_id,
        genesis_challenge,
    ))
}
