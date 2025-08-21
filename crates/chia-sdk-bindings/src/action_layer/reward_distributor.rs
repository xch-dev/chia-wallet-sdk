use std::sync::{Arc, Mutex};

use chia_protocol::Bytes32;
use chia_sdk_driver::{
    RewardDistributor as SdkRewardDistributor, RewardDistributorConstants, RewardDistributorType,
    SpendContext,
};

pub trait RewardDistributorTypeExt {}

impl RewardDistributorTypeExt for RewardDistributorType {}

pub trait RewardDistributorConstantsExt {
    #[allow(clippy::too_many_arguments)]
    fn without_launcher_id(
        reward_distributor_type: RewardDistributorType,
        manager_or_collection_did_launcher_id: Bytes32,
        fee_payout_puzzle_hash: Bytes32,
        epoch_seconds: u64,
        max_seconds_offset: u64,
        payout_threshold: u64,
        fee_bps: u64,
        withdrawal_share_bps: u64,
        reserve_asset_id: Bytes32,
    ) -> Self;

    fn with_launcher_id(self, launcher_id: Bytes32) -> Self;
}

impl RewardDistributorConstantsExt for RewardDistributorConstants {
    #[allow(clippy::too_many_arguments)]
    fn without_launcher_id(
        reward_distributor_type: RewardDistributorType,
        manager_or_collection_did_launcher_id: Bytes32,
        fee_payout_puzzle_hash: Bytes32,
        epoch_seconds: u64,
        max_seconds_offset: u64,
        payout_threshold: u64,
        fee_bps: u64,
        withdrawal_share_bps: u64,
        reserve_asset_id: Bytes32,
    ) -> Self {
        RewardDistributorConstants::without_launcher_id(
            reward_distributor_type,
            manager_or_collection_did_launcher_id,
            fee_payout_puzzle_hash,
            epoch_seconds,
            max_seconds_offset,
            payout_threshold,
            fee_bps,
            withdrawal_share_bps,
            reserve_asset_id,
        )
    }

    fn with_launcher_id(self, launcher_id: Bytes32) -> Self {
        RewardDistributorConstants::with_launcher_id(self, launcher_id)
    }
}

#[derive(Clone)]
pub struct RewardDistributor {
    pub(crate) clvm: Arc<Mutex<SpendContext>>,
    pub(crate) distributor: Arc<Mutex<SdkRewardDistributor>>,
}

impl RewardDistributor {}
