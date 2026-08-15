use std::sync::{Arc, Mutex};

use bindy::{Error, Result};
use chia_bls::{SecretKey, Signature};
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{LineageProof, singleton::SingletonStruct};
use chia_sdk_driver::{
    Cat, Reserve, RewardDistributor as SdkRewardDistributor,
    RewardDistributorActionLog as SdkRewardDistributorActionLog, RewardDistributorAddEntryAction,
    RewardDistributorAddEntryActionLog, RewardDistributorAddIncentivesAction,
    RewardDistributorAddIncentivesActionLog, RewardDistributorCommitIncentivesAction,
    RewardDistributorCommitIncentivesActionLog, RewardDistributorConstants,
    RewardDistributorInitiatePayoutAction, RewardDistributorInitiatePayoutActionLog,
    RewardDistributorNewEpochAction, RewardDistributorNewEpochActionLog,
    RewardDistributorRefreshAction, RewardDistributorRefreshNftsFromDlActionLog,
    RewardDistributorRemoveEntryAction, RewardDistributorRemoveEntryActionLog,
    RewardDistributorStakeAction, RewardDistributorStakeActionLog, RewardDistributorState,
    RewardDistributorSyncAction, RewardDistributorSyncActionLog,
    RewardDistributorType as SdkRewardDistributorType, RewardDistributorUnstakeAction,
    RewardDistributorUnstakeActionLog, RewardDistributorWithdrawIncentivesAction,
    RewardDistributorWithdrawIncentivesActionLog, RoundRewardInfo, RoundTimeInfo, SpendContext,
};
use chia_sdk_types::{
    Conditions, MerkleProof, Mod,
    puzzles::{
        IntermediaryCoinProof, NftLauncherProof, NonceWrapperArgs, RewardDistributorSlotNonce,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{
    AsProgram, AsPtr, CatSpend, CommitmentSlot, EntrySlot, Nft, NotarizedPayment, Program, Proof,
    RewardSlot,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RewardDistributorType(pub SdkRewardDistributorType);

impl RewardDistributorType {
    pub fn managed(manager_singleton_launcher_id: Bytes32) -> Result<Self> {
        Ok(Self(SdkRewardDistributorType::Managed {
            manager_singleton_launcher_id,
        }))
    }

    pub fn nft_collection(collection_did_launcher_id: Bytes32) -> Result<Self> {
        Ok(Self(SdkRewardDistributorType::NftCollection {
            collection_did_launcher_id,
        }))
    }

    pub fn curated_nft(store_launcher_id: Bytes32, refreshable: bool) -> Result<Self> {
        Ok(Self(SdkRewardDistributorType::CuratedNft {
            store_launcher_id,
            refreshable,
        }))
    }

    pub fn cat(asset_id: Bytes32, hidden_puzzle_hash: Option<Bytes32>) -> Result<Self> {
        Ok(Self(SdkRewardDistributorType::Cat {
            asset_id,
            hidden_puzzle_hash,
        }))
    }
}

impl<T, L> bindy::FromRust<SdkRewardDistributorType, T, L> for RewardDistributorType {
    fn from_rust(value: SdkRewardDistributorType, _context: &T) -> bindy::Result<Self> {
        Ok(Self(value))
    }
}

impl<T, L> bindy::IntoRust<SdkRewardDistributorType, T, L> for RewardDistributorType {
    fn into_rust(self, _context: &T) -> bindy::Result<SdkRewardDistributorType> {
        Ok(self.0)
    }
}

pub trait RewardDistributorConstantsExt
where
    Self: Sized,
{
    #[allow(clippy::too_many_arguments)]
    fn without_launcher_id(
        reward_distributor_type: RewardDistributorType,
        fee_payout_puzzle_hash: Bytes32,
        epoch_seconds: u64,
        precision: u64,
        max_seconds_offset: u64,
        payout_threshold: u64,
        require_payout_approval: bool,
        fee_bps: u64,
        withdrawal_share_bps: u64,
        reserve_asset_id: Bytes32,
    ) -> Result<Self>;

    fn with_launcher_id(&self, launcher_id: Bytes32) -> Result<Self>;

    fn reward_distributor_type(&self) -> Result<RewardDistributorType>;
}

impl RewardDistributorConstantsExt for RewardDistributorConstants {
    #[allow(clippy::too_many_arguments)]
    fn without_launcher_id(
        reward_distributor_type: RewardDistributorType,
        fee_payout_puzzle_hash: Bytes32,
        epoch_seconds: u64,
        precision: u64,
        max_seconds_offset: u64,
        payout_threshold: u64,
        require_payout_approval: bool,
        fee_bps: u64,
        withdrawal_share_bps: u64,
        reserve_asset_id: Bytes32,
    ) -> Result<Self> {
        Ok(RewardDistributorConstants::without_launcher_id(
            reward_distributor_type.0,
            fee_payout_puzzle_hash,
            epoch_seconds,
            precision,
            max_seconds_offset,
            payout_threshold,
            require_payout_approval,
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

    fn reward_distributor_type(&self) -> Result<RewardDistributorType> {
        Ok(RewardDistributorType(self.reward_distributor_type))
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
pub struct RewardDistributorStakeCollectionNftsResult {
    pub conditions: Vec<Program>,
    pub notarized_payments: Vec<NotarizedPayment>,
    pub new_nfts: Vec<Nft>,
}

#[derive(Clone)]
pub struct RewardDistributorStakeCuratedNftsResult {
    pub conditions: Vec<Program>,
    pub notarized_payments: Vec<NotarizedPayment>,
    pub new_nfts: Vec<Nft>,
}

#[derive(Clone)]
pub struct RewardDistributorStakeCatResult {
    pub conditions: Vec<Program>,
    pub notarized_payment: NotarizedPayment,
    pub new_cat: Cat,
}

#[derive(Clone)]
pub struct RewardDistributorUnstakeLockedNftsResult {
    pub conditions: Vec<Program>,
    pub payment_amount: u64,
}

#[derive(Clone)]
pub struct RewardDistributorUnstakeLockedCatResult {
    pub conditions: Vec<Program>,
    pub payment_amount: u64,
}

#[derive(Clone)]
pub struct RewardDistributorRefreshNftsResult {
    pub conditions: Vec<Program>,
    pub new_nfts: Vec<Nft>,
}

#[derive(Clone)]
pub struct RefreshNftsInfo {
    pub slot: EntrySlot,
    pub nfts: Vec<Nft>,
    pub nft_shares_delta: Vec<i64>,
    pub new_shares: Vec<u64>,
    pub nft_inclusion_proofs: Vec<MerkleProof>,
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
pub struct RewardDistributorActionLog {
    pub kind: String,
    pub add_entry: Option<RewardDistributorAddEntryActionLog>,
    pub remove_entry: Option<RewardDistributorRemoveEntryActionLog>,
    pub add_incentives: Option<RewardDistributorAddIncentivesActionLog>,
    pub commit_incentives: Option<RewardDistributorCommitIncentivesActionLog>,
    pub initiate_payout: Option<RewardDistributorInitiatePayoutActionLog>,
    pub new_epoch: Option<RewardDistributorNewEpochActionLog>,
    pub sync: Option<RewardDistributorSyncActionLog>,
    pub withdraw_incentives: Option<RewardDistributorWithdrawIncentivesActionLog>,
    pub refresh_nfts_from_dl: Option<RewardDistributorRefreshNftsFromDlActionLog>,
    pub stake: Option<RewardDistributorStakeActionLog>,
    pub unstake: Option<RewardDistributorUnstakeActionLog>,
}

impl From<SdkRewardDistributorActionLog> for RewardDistributorActionLog {
    fn from(log: SdkRewardDistributorActionLog) -> Self {
        let mut result = Self {
            kind: String::new(),
            add_entry: None,
            remove_entry: None,
            add_incentives: None,
            commit_incentives: None,
            initiate_payout: None,
            new_epoch: None,
            sync: None,
            withdraw_incentives: None,
            refresh_nfts_from_dl: None,
            stake: None,
            unstake: None,
        };

        match log {
            SdkRewardDistributorActionLog::AddEntry(payload) => {
                result.kind = "AddEntry".to_string();
                result.add_entry = Some(payload);
            }
            SdkRewardDistributorActionLog::RemoveEntry(payload) => {
                result.kind = "RemoveEntry".to_string();
                result.remove_entry = Some(payload);
            }
            SdkRewardDistributorActionLog::AddIncentives(payload) => {
                result.kind = "AddIncentives".to_string();
                result.add_incentives = Some(payload);
            }
            SdkRewardDistributorActionLog::CommitIncentives(payload) => {
                result.kind = "CommitIncentives".to_string();
                result.commit_incentives = Some(payload);
            }
            SdkRewardDistributorActionLog::InitiatePayout(payload) => {
                result.kind = "InitiatePayout".to_string();
                result.initiate_payout = Some(payload);
            }
            SdkRewardDistributorActionLog::NewEpoch(payload) => {
                result.kind = "NewEpoch".to_string();
                result.new_epoch = Some(payload);
            }
            SdkRewardDistributorActionLog::Sync(payload) => {
                result.kind = "Sync".to_string();
                result.sync = Some(payload);
            }
            SdkRewardDistributorActionLog::WithdrawIncentives(payload) => {
                result.kind = "WithdrawIncentives".to_string();
                result.withdraw_incentives = Some(payload);
            }
            SdkRewardDistributorActionLog::RefreshNftsFromDl(payload) => {
                result.kind = "RefreshNftsFromDl".to_string();
                result.refresh_nfts_from_dl = Some(payload);
            }
            SdkRewardDistributorActionLog::Stake(payload) => {
                result.kind = "Stake".to_string();
                result.stake = Some(payload);
            }
            SdkRewardDistributorActionLog::Unstake(payload) => {
                result.kind = "Unstake".to_string();
                result.unstake = Some(payload);
            }
        }

        result
    }
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
        // Pending actions (including those reconstructed from a mempool item) update
        // `pending_spend.latest_state`. Builders must see that tip — not the coin's
        // pre-spend `info.state`.
        Ok(self
            .distributor
            .lock()
            .unwrap()
            .pending_spend
            .latest_state
            .1)
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

    pub fn pending_logs(&self) -> Result<Vec<RewardDistributorActionLog>> {
        Ok(self
            .distributor
            .lock()
            .unwrap()
            .pending_spend
            .logs
            .clone()
            .into_iter()
            .map(Into::into)
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

        if let SdkRewardDistributorType::Managed { .. } =
            distributor.info.constants.reward_distributor_type
        {
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
        } else {
            Err(Error::Custom(
                "Reward distributor is not managed".to_string(),
            ))
        }
    }

    pub fn remove_entry(
        &self,
        entry_slot: EntrySlot,
        manager_singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<RewardDistributorRemoveEntryResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        if let SdkRewardDistributorType::Managed { .. } =
            distributor.info.constants.reward_distributor_type
        {
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
        } else {
            Err(Error::Custom(
                "Reward distributor is not managed".to_string(),
            ))
        }
    }

    pub fn stake_collection_nfts(
        &self,
        offered_nfts: Vec<Nft>,
        nft_launcher_proofs: Vec<NftLauncherProof>,
        entry_custody_puzzle_hash: Bytes32,
        existing_slot: Option<EntrySlot>,
    ) -> Result<RewardDistributorStakeCollectionNftsResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let sdk_nfts: Vec<_> = offered_nfts.iter().map(|nft| nft.as_ptr(&ctx)).collect();
        let (conditions, notarized_payments, new_nfts) = distributor
            .new_action::<RewardDistributorStakeAction>()
            .spend_for_collection_nft_mode(
                &mut ctx,
                &mut distributor,
                &sdk_nfts,
                &nft_launcher_proofs,
                entry_custody_puzzle_hash,
                existing_slot.map(EntrySlot::to_slot),
            )?;

        Ok(RewardDistributorStakeCollectionNftsResult {
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
            notarized_payments: notarized_payments
                .iter()
                .map(|np| np.as_program(&self.clvm))
                .collect(),
            new_nfts: new_nfts
                .iter()
                .map(|nft| nft.as_program(&self.clvm))
                .collect(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stake_curated_nfts(
        &self,
        offered_nfts: Vec<Nft>,
        nft_shares: Vec<u64>,
        inclusion_proofs: Vec<MerkleProof>,
        entry_custody_puzzle_hash: Bytes32,
        existing_slot: Option<EntrySlot>,
        dl_root_hash: Bytes32,
        dl_metadata_rest_hash: Option<Bytes32>,
        dl_metadata_updater_hash_hash: Bytes32,
        dl_inner_puzzle_hash: Bytes32,
    ) -> Result<RewardDistributorStakeCuratedNftsResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let sdk_nfts: Vec<_> = offered_nfts.iter().map(|nft| nft.as_ptr(&ctx)).collect();
        let (conditions, notarized_payments, new_nfts) = distributor
            .new_action::<RewardDistributorStakeAction>()
            .spend_for_curated_nft_mode(
                &mut ctx,
                &mut distributor,
                &sdk_nfts,
                &nft_shares,
                &inclusion_proofs,
                entry_custody_puzzle_hash,
                existing_slot.map(EntrySlot::to_slot),
                dl_root_hash,
                dl_metadata_rest_hash,
                dl_metadata_updater_hash_hash,
                dl_inner_puzzle_hash,
            )?;

        Ok(RewardDistributorStakeCuratedNftsResult {
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
            notarized_payments: notarized_payments
                .iter()
                .map(|np| np.as_program(&self.clvm))
                .collect(),
            new_nfts: new_nfts
                .iter()
                .map(|nft| nft.as_program(&self.clvm))
                .collect(),
        })
    }

    pub fn stake_cat(
        &self,
        offered_cat: Cat,
        entry_custody_puzzle_hash: Bytes32,
        existing_slot: Option<EntrySlot>,
    ) -> Result<RewardDistributorStakeCatResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let (conditions, notarized_payment, new_cat) = distributor
            .new_action::<RewardDistributorStakeAction>()
            .spend_for_cat_mode(
                &mut ctx,
                &mut distributor,
                offered_cat,
                entry_custody_puzzle_hash,
                existing_slot.map(EntrySlot::to_slot),
            )?;

        Ok(RewardDistributorStakeCatResult {
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
            notarized_payment: notarized_payment.as_program(&self.clvm),
            new_cat,
        })
    }

    pub fn unstake_locked_nfts(
        &self,
        entry_slot: EntrySlot,
        locked_nfts: Vec<Nft>,
        locked_nft_shares: Vec<u64>,
    ) -> Result<RewardDistributorUnstakeLockedNftsResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let sdk_locked_nfts: Vec<_> = locked_nfts.iter().map(|nft| nft.as_ptr(&ctx)).collect();
        let (conditions, payment_amount) = distributor
            .new_action::<RewardDistributorUnstakeAction>()
            .spend_for_locked_nfts(
                &mut ctx,
                &mut distributor,
                entry_slot.to_slot(),
                &sdk_locked_nfts,
                &locked_nft_shares,
            )?;

        Ok(RewardDistributorUnstakeLockedNftsResult {
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
            payment_amount,
        })
    }

    pub fn unstake_locked_cat(
        &self,
        entry_slot: EntrySlot,
        locked_cat: Cat,
    ) -> Result<RewardDistributorUnstakeLockedCatResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let (conditions, payment_amount) = distributor
            .new_action::<RewardDistributorUnstakeAction>()
            .spend_for_locked_cats(&mut ctx, &mut distributor, entry_slot.to_slot(), locked_cat)?;

        Ok(RewardDistributorUnstakeLockedCatResult {
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
            payment_amount,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn refresh_nfts(
        &self,
        refresh_nfts_infos: Vec<RefreshNftsInfo>,
        dl_root_hash: Bytes32,
        dl_metadata_rest_hash: Option<Bytes32>,
        dl_metadata_updater_hash_hash: Bytes32,
        dl_inner_puzzle_hash: Bytes32,
    ) -> Result<RewardDistributorRefreshNftsResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut distributor = self.distributor.lock().unwrap();

        let slots: Vec<_> = refresh_nfts_infos
            .iter()
            .map(|info| info.slot.clone().to_slot())
            .collect();

        let sdk_nft_groups: Vec<Vec<_>> = refresh_nfts_infos
            .iter()
            .map(|info| info.nfts.iter().map(|nft| nft.as_ptr(&ctx)).collect())
            .collect();
        let sdk_nft_refs: Vec<&[chia_sdk_driver::Nft]> =
            sdk_nft_groups.iter().map(Vec::as_slice).collect();
        let shares_delta_refs: Vec<&[i64]> = refresh_nfts_infos
            .iter()
            .map(|info| info.nft_shares_delta.as_slice())
            .collect();
        let new_shares_refs: Vec<&[u64]> = refresh_nfts_infos
            .iter()
            .map(|info| info.new_shares.as_slice())
            .collect();
        let inclusion_proof_refs: Vec<&[MerkleProof]> = refresh_nfts_infos
            .iter()
            .map(|info| info.nft_inclusion_proofs.as_slice())
            .collect();

        let (conditions, new_nfts) = distributor
            .new_action::<RewardDistributorRefreshAction>()
            .spend(
                &mut ctx,
                &mut distributor,
                slots,
                &sdk_nft_refs,
                &shares_delta_refs,
                &new_shares_refs,
                &inclusion_proof_refs,
                dl_root_hash,
                dl_metadata_rest_hash,
                dl_metadata_updater_hash_hash,
                dl_inner_puzzle_hash,
            )?;

        Ok(RewardDistributorRefreshNftsResult {
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
            new_nfts: new_nfts
                .iter()
                .map(|nft| nft.as_program(&self.clvm))
                .collect(),
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
