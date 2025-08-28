use std::sync::{Arc, Mutex};

use bindy::{Error, Result};
use chia_bls::{SecretKey, Signature};
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{singleton::SingletonStruct, LineageProof};
use chia_sdk_driver::{
    Cat, Reserve, RewardDistributor as SdkRewardDistributor, RewardDistributorAddEntryAction,
    RewardDistributorAddIncentivesAction, RewardDistributorCommitIncentivesAction,
    RewardDistributorConstants, RewardDistributorInitiatePayoutAction,
    RewardDistributorNewEpochAction, RewardDistributorRemoveEntryAction,
    RewardDistributorStakeAction, RewardDistributorState, RewardDistributorSyncAction,
    RewardDistributorType, RewardDistributorUnstakeAction,
    RewardDistributorWithdrawIncentivesAction, RoundRewardInfo, RoundTimeInfo, SpendContext,
};
use chia_sdk_types::{
    puzzles::{
        IntermediaryCoinProof, NftLauncherProof, NonceWrapperArgs, RewardDistributorSlotNonce,
    },
    Conditions, Mod,
};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{
    AsProgram, AsPtr, CatSpend, CommitmentSlot, EntrySlot, Nft, NotarizedPayment, Program, Proof,
    RewardSlot,
};

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

pub trait IntermediaryCoinProofExt {}

impl IntermediaryCoinProofExt for IntermediaryCoinProof {}

pub trait NftLauncherProofExt {}

impl NftLauncherProofExt for NftLauncherProof {}

#[derive(Clone)]
pub struct RewardDistributorStakeResult {
    pub conditions: Vec<Program>,
    pub notarized_payment: NotarizedPayment,
    pub new_nft: Nft,
}

#[derive(Clone)]
pub struct RewardDistributorUnstakeResult {
    pub conditions: Vec<Program>,
    pub payment_amount: u64,
}

#[derive(Clone)]
pub struct RewardDistributorLaunchResult {
    pub security_signature: Signature,
    pub security_secret_key: SecretKey,
    pub reward_distributor: RewardDistributor,
    pub first_epoch_slot: RewardSlot,
    pub refunded_cat: Cat,
}

#[derive(Clone)]
pub struct RewardDistributorInfoFromLauncher {
    pub constants: RewardDistributorConstants,
    pub initial_state: RewardDistributorState,
    pub eve_singleton: Coin,
}

#[derive(Clone)]
pub struct RewardDistributorInfoFromEveCoin {
    pub distributor: RewardDistributor,
    pub first_reward_slot: RewardSlot,
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

    pub fn reserve_coin(&self) -> Result<Coin> {
        Ok(self.distributor.lock().unwrap().reserve.coin)
    }

    pub fn reserve_asset_id(&self) -> Result<Bytes32> {
        Ok(self.distributor.lock().unwrap().reserve.asset_id)
    }

    pub fn reserve_proof(&self) -> Result<LineageProof> {
        Ok(self.distributor.lock().unwrap().reserve.proof)
    }

    pub fn pending_created_reward_slots(&self) -> Result<Vec<RewardSlot>> {
        let distributor = self.distributor.lock().unwrap();

        Ok(distributor
            .pending_spend
            .created_reward_slots
            .clone()
            .into_iter()
            .map(|slot_value| {
                RewardSlot::from_slot(
                    distributor
                        .created_slot_value_to_slot(slot_value, RewardDistributorSlotNonce::REWARD),
                )
            })
            .collect())
    }

    pub fn pending_created_commitment_slots(&self) -> Result<Vec<CommitmentSlot>> {
        let distributor = self.distributor.lock().unwrap();

        Ok(distributor
            .pending_spend
            .created_commitment_slots
            .clone()
            .into_iter()
            .map(|slot_value| {
                CommitmentSlot::from_slot(
                    distributor.created_slot_value_to_slot(
                        slot_value,
                        RewardDistributorSlotNonce::COMMITMENT,
                    ),
                )
            })
            .collect())
    }

    pub fn pending_created_entry_slots(&self) -> Result<Vec<EntrySlot>> {
        let distributor = self.distributor.lock().unwrap();

        Ok(distributor
            .pending_spend
            .created_entry_slots
            .clone()
            .into_iter()
            .map(|slot_value| {
                EntrySlot::from_slot(
                    distributor
                        .created_slot_value_to_slot(slot_value, RewardDistributorSlotNonce::ENTRY),
                )
            })
            .collect())
    }

    pub fn pending_signature(&self) -> Result<Signature> {
        Ok(self
            .distributor
            .lock()
            .unwrap()
            .pending_spend
            .signature
            .clone())
    }

    pub fn reserve_full_puzzle_hash(
        asset_id: Bytes32,
        distributor_launcher_id: Bytes32,
        nonce: u64,
    ) -> Result<TreeHash> {
        Ok(Reserve::puzzle_hash(
            asset_id,
            SingletonStruct::new(distributor_launcher_id)
                .tree_hash()
                .into(),
            nonce,
        ))
    }

    pub fn parse_launcher_solution(
        launcher_coin: Coin,
        launcher_solution: Program,
    ) -> Result<Option<RewardDistributorInfoFromLauncher>> {
        let mut ctx = launcher_solution.0.lock().unwrap();

        Ok(SdkRewardDistributor::from_launcher_solution(
            &mut ctx,
            launcher_coin,
            launcher_solution.1,
        )?
        .map(|(constants, initial_state, eve_singleton)| {
            RewardDistributorInfoFromLauncher {
                constants,
                initial_state,
                eve_singleton,
            }
        }))
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

    fn sdk_conditions_to_program_list(
        &self,
        ctx: &mut SpendContext,
        conditions: Conditions,
    ) -> Result<Vec<Program>> {
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

        self.sdk_conditions_to_program_list(&mut ctx, conditions)
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

        self.sdk_conditions_to_program_list(&mut ctx, conditions)
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
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
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
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
            epoch_fee,
        })
    }

    pub fn sync(&self, update_time: u64) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let conditions = distributor
            .new_action::<RewardDistributorSyncAction>()
            .spend(&mut ctx, &mut distributor, update_time)?;

        self.sdk_conditions_to_program_list(&mut ctx, conditions)
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
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
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
                "Reward distributor is not managed".to_string(),
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

        self.sdk_conditions_to_program_list(&mut ctx, conditions)
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
                "Reward distributor is not managed".to_string(),
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
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
            last_payment_amount,
        })
    }

    pub fn stake(
        &self,
        current_nft: Nft,
        nft_launcher_proof: NftLauncherProof,
        entry_custody_puzzle_hash: Bytes32,
    ) -> Result<RewardDistributorStakeResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        if distributor.info.constants.reward_distributor_type != RewardDistributorType::Nft {
            return Err(Error::Custom(
                "Reward distributor is not an NFT one".to_string(),
            ));
        }

        let sdk_nft = current_nft.as_ptr(&ctx);
        let (conditions, notarized_payment, new_nft) = distributor
            .new_action::<RewardDistributorStakeAction>()
            .spend(
                &mut ctx,
                &mut distributor,
                sdk_nft,
                nft_launcher_proof,
                entry_custody_puzzle_hash,
            )?;

        Ok(RewardDistributorStakeResult {
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
            notarized_payment: notarized_payment.as_program(&self.clvm),
            new_nft: new_nft.as_program(&self.clvm),
        })
    }

    pub fn unstake(
        &self,
        entry_slot: EntrySlot,
        locked_nft: Nft,
    ) -> Result<RewardDistributorUnstakeResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let sdk_locked_nft = locked_nft.as_ptr(&ctx);
        let (conditions, payment_amount) = distributor
            .new_action::<RewardDistributorUnstakeAction>()
            .spend(
                &mut ctx,
                &mut distributor,
                entry_slot.to_slot(),
                sdk_locked_nft,
            )?;

        Ok(RewardDistributorUnstakeResult {
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
            payment_amount,
        })
    }

    pub fn locked_nft_hint(
        distributor_launcher_id: Bytes32,
        custody_puzzle_hash: Bytes32,
    ) -> Result<Bytes32> {
        Ok(NonceWrapperArgs::<Bytes32, TreeHash> {
            nonce: custody_puzzle_hash,
            inner_puzzle: RewardDistributorStakeAction::my_p2_puzzle_hash(distributor_launcher_id)
                .into(),
        }
        .curry_tree_hash()
        .into())
    }
}
