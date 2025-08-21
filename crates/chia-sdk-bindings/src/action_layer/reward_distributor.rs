use std::sync::{Arc, Mutex};

use bindy::{Error, Result};
use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::LineageProof;
use chia_sdk_driver::{
    Reserve, RewardDistributor as SdkRewardDistributor, RewardDistributorAddEntryAction,
    RewardDistributorAddIncentivesAction, RewardDistributorCommitIncentivesAction,
    RewardDistributorConstants, RewardDistributorInitiatePayoutAction,
    RewardDistributorNewEpochAction, RewardDistributorRemoveEntryAction, RewardDistributorState,
    RewardDistributorSyncAction, RewardDistributorType, RewardDistributorWithdrawIncentivesAction,
    RoundRewardInfo, RoundTimeInfo, SpendContext,
};
use chia_sdk_types::Conditions;
use clvm_utils::TreeHash;

use crate::{CatSpend, CommitmentSlot, EntrySlot, Program, Proof, RewardSlot};

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
pub struct RewardDistributorFinishedSpendResult {
    pub new_distributor: RewardDistributor,
    pub signature: Signature,
}

#[derive(Clone)]
pub struct RewardDistributor {
    pub(crate) clvm: Arc<Mutex<SpendContext>>,
    pub(crate) distributor: Arc<Mutex<SdkRewardDistributor>>,
}

#[derive(Clone)]
pub struct RewardDistributorInitiatePayoutResult {
    pub conditions: Vec<Program>,
    pub payout_amount: u64,
}

#[derive(Clone)]
pub struct RewardDistributorNewEpochResult {
    pub conditions: Vec<Program>,
    pub epoch_fee: u64,
}

#[derive(Clone)]
pub struct RewardDistributorWithdrawIncentivesResult {
    pub conditions: Vec<Program>,
    pub withdrawn_amount: u64,
}

#[derive(Clone)]
pub struct RewardDistributorRemoveEntryResult {
    pub conditions: Vec<Program>,
    pub last_payment_amount: u64,
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

    pub fn reserve_coin(&self) -> Result<Coin> {
        Ok(self.distributor.lock().unwrap().reserve.coin)
    }

    pub fn reserve_asset_id(&self) -> Result<Bytes32> {
        Ok(self.distributor.lock().unwrap().reserve.asset_id)
    }

    pub fn reserve_proof(&self) -> Result<LineageProof> {
        Ok(self.distributor.lock().unwrap().reserve.proof)
    }

    pub fn reserve_full_puzzle_hash(
        asset_id: Bytes32,
        controller_singleton_struct_hash: Bytes32,
        nonce: u64,
    ) -> Result<TreeHash> {
        Ok(Reserve::puzzle_hash(
            asset_id,
            controller_singleton_struct_hash,
            nonce,
        ))
    }

    pub fn finish_spend(
        &self,
        other_cat_spends: Vec<CatSpend>,
    ) -> Result<RewardDistributorFinishedSpendResult> {
        let mut ctx = self.clvm.lock().unwrap();

        let (distributor, signature) = self.distributor.lock().unwrap().clone().finish_spend(
            &mut ctx,
            other_cat_spends.into_iter().map(Into::into).collect(),
        )?;

        Ok(RewardDistributorFinishedSpendResult {
            new_distributor: RewardDistributor {
                clvm: self.clvm.clone(),
                distributor: Arc::new(Mutex::new(distributor)),
            },
            signature,
        })
    }

    fn sdk_conditions_to_program_list(&self, conditions: Conditions) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut result = Vec::with_capacity(conditions.len());

        for condition in conditions {
            result.push(Program(self.clvm.clone(), ctx.alloc(&condition)?));
        }

        Ok(result)
    }

    pub fn add_incentives(&self, amount: u64) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let conditions = distributor
            .new_action::<RewardDistributorAddIncentivesAction>()
            .spend(&mut ctx, &mut distributor, amount)?;

        self.sdk_conditions_to_program_list(conditions)
    }

    pub fn commit_incentives(
        &self,
        reward_slot: RewardSlot,
        epoch_start: u64,
        clawback_ph: Bytes32,
        rewards_to_add: u64,
    ) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let conditions = distributor
            .new_action::<RewardDistributorCommitIncentivesAction>()
            .spend(
                &mut ctx,
                &mut distributor,
                reward_slot.to_slot(),
                epoch_start,
                clawback_ph,
                rewards_to_add,
            )?;

        self.sdk_conditions_to_program_list(conditions)
    }

    pub fn initiate_payout(
        &self,
        entry_slot: EntrySlot,
    ) -> Result<RewardDistributorInitiatePayoutResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let (conditions, payout_amount) = distributor
            .new_action::<RewardDistributorInitiatePayoutAction>()
            .spend(&mut ctx, &mut distributor, entry_slot.to_slot())?;

        Ok(RewardDistributorInitiatePayoutResult {
            conditions: self.sdk_conditions_to_program_list(conditions)?,
            payout_amount,
        })
    }

    pub fn new_epoch(&self, reward_slot: RewardSlot) -> Result<RewardDistributorNewEpochResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let (conditions, epoch_fee) = distributor
            .new_action::<RewardDistributorNewEpochAction>()
            .spend(&mut ctx, &mut distributor, reward_slot.to_slot())?;

        Ok(RewardDistributorNewEpochResult {
            conditions: self.sdk_conditions_to_program_list(conditions)?,
            epoch_fee,
        })
    }

    pub fn sync(&self, update_time: u64) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let conditions = distributor
            .new_action::<RewardDistributorSyncAction>()
            .spend(&mut ctx, &mut distributor, update_time)?;

        self.sdk_conditions_to_program_list(conditions)
    }

    pub fn withdraw_incentives(
        &self,
        commitment_slot: CommitmentSlot,
        reward_slot: RewardSlot,
    ) -> Result<RewardDistributorWithdrawIncentivesResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let (conditions, withdrawn_amount) = distributor
            .new_action::<RewardDistributorWithdrawIncentivesAction>()
            .spend(
                &mut ctx,
                &mut distributor,
                commitment_slot.to_slot(),
                reward_slot.to_slot(),
            )?;

        Ok(RewardDistributorWithdrawIncentivesResult {
            conditions: self.sdk_conditions_to_program_list(conditions)?,
            withdrawn_amount,
        })
    }

    pub fn add_entry(
        &self,
        payout_puzzle_hash: Bytes32,
        shares: u64,
        manager_singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        if distributor.info.constants.reward_distributor_type != RewardDistributorType::Manager {
            return Err(Error::Custom(
                "Reward distributor is not a manager one".to_string(),
            ));
        }

        let conditions = distributor
            .new_action::<RewardDistributorAddEntryAction>()
            .spend(
                &mut ctx,
                &mut distributor,
                payout_puzzle_hash,
                shares,
                manager_singleton_inner_puzzle_hash,
            )?;

        self.sdk_conditions_to_program_list(conditions)
    }

    pub fn remove_entry(
        &self,
        entry_slot: EntrySlot,
        manager_singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<RewardDistributorRemoveEntryResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        if distributor.info.constants.reward_distributor_type != RewardDistributorType::Manager {
            return Err(Error::Custom(
                "Reward distributor is not a manager one".to_string(),
            ));
        }

        let (conditions, last_payment_amount) = distributor
            .new_action::<RewardDistributorRemoveEntryAction>()
            .spend(
                &mut ctx,
                &mut distributor,
                entry_slot.to_slot(),
                manager_singleton_inner_puzzle_hash,
            )?;

        Ok(RewardDistributorRemoveEntryResult {
            conditions: self.sdk_conditions_to_program_list(conditions)?,
            last_payment_amount,
        })
    }
}
