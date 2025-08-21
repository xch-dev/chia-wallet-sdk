use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin};
use chia_sdk_driver::{
    RewardDistributor as SdkRewardDistributor, RewardDistributorConstants, RewardDistributorState,
    RewardDistributorType, RoundRewardInfo, RoundTimeInfo, SpendContext,
};
use clvm_utils::TreeHash;

use crate::{CatSpend, Proof};

pub trait RewardDistributorTypeExt {}

impl RewardDistributorTypeExt for RewardDistributorType {}

pub trait RewardDistributorConstantsExt
where
    Self: Sized,
{
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
    ) -> Result<Self>;

    fn with_launcher_id(&self, launcher_id: Bytes32) -> Result<Self>;
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
    ) -> Result<Self> {
        Ok(RewardDistributorConstants::without_launcher_id(
            reward_distributor_type,
            manager_or_collection_did_launcher_id,
            fee_payout_puzzle_hash,
            epoch_seconds,
            max_seconds_offset,
            payout_threshold,
            fee_bps,
            withdrawal_share_bps,
            reserve_asset_id,
        ))
    }

    fn with_launcher_id(&self, launcher_id: Bytes32) -> Result<Self> {
        Ok(RewardDistributorConstants::with_launcher_id(
            *self,
            launcher_id,
        ))
    }
}

pub trait RoundRewardInfoExt {}

impl RoundRewardInfoExt for RoundRewardInfo {}

pub trait RoundTimeInfoExt {}

impl RoundTimeInfoExt for RoundTimeInfo {}

pub trait RewardDistributorStateExt
where
    Self: Sized,
{
    fn initial(first_epoch_start: u64) -> Result<Self>;
}

impl RewardDistributorStateExt for RewardDistributorState {
    fn initial(first_epoch_start: u64) -> Result<Self> {
        Ok(RewardDistributorState::initial(first_epoch_start))
    }
}

pub trait RewardDistributorLauncherSolutionInfoExt {}

impl RewardDistributorLauncherSolutionInfoExt for RewardDistributorLauncherSolutionInfo {}

#[derive(Clone, Copy)]
pub struct RewardDistributorLauncherSolutionInfo {
    pub constants: RewardDistributorConstants,
    pub initial_state: RewardDistributorState,
    pub coin: Coin,
}

#[derive(Clone)]
pub struct RewardDistributor {
    pub(crate) clvm: Arc<Mutex<SpendContext>>,
    pub(crate) distributor: Arc<Mutex<SdkRewardDistributor>>,
}

impl RewardDistributor {
    pub fn coin(&self) -> Result<Coin> {
        Ok(self.distributor.lock().unwrap().coin)
    }

    pub fn proof(&self) -> Result<Proof> {
        Ok(self.distributor.lock().unwrap().proof.into())
    }

    pub fn state(&self) -> Result<RewardDistributorState> {
        Ok(self.distributor.lock().unwrap().info.state)
    }

    pub fn constants(&self) -> Result<RewardDistributorConstants> {
        Ok(self.distributor.lock().unwrap().info.constants)
    }

    pub fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.distributor.lock().unwrap().info.inner_puzzle_hash())
    }

    pub fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.distributor.lock().unwrap().info.puzzle_hash())
    }

    pub fn finish_spend(
        &self,
        other_cat_spends: Vec<CatSpend>,
    ) -> Result<(RewardDistributor, Signature)> {
        let mut ctx = self.clvm.lock().unwrap();

        let (distributor, signature) = self.distributor.lock().unwrap().clone().finish_spend(
            &mut ctx,
            other_cat_spends.into_iter().map(Into::into).collect(),
        )?;

        Ok((
            RewardDistributor {
                clvm: self.clvm.clone(),
                distributor: Arc::new(Mutex::new(distributor)),
            },
            signature,
        ))
    }
}
