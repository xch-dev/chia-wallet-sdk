use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::{
    RewardDistributorCommitmentSlotValue, RewardDistributorEntrySlotValue,
    RewardDistributorRewardSlotValue,
};

use crate::RewardDistributorState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorNftStakeEntry {
    pub launcher_id: Bytes32,
    pub shares: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorStateTransition {
    pub old_state: RewardDistributorState,
    pub new_state: RewardDistributorState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorAddEntryActionLog {
    pub created_entry_slot: RewardDistributorEntrySlotValue,
    pub manager_singleton_inner_puzzle_hash: Bytes32,
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorRemoveEntryActionLog {
    pub spent_entry_slot: RewardDistributorEntrySlotValue,
    pub manager_singleton_inner_puzzle_hash: Bytes32,
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorAddIncentivesActionLog {
    pub amount: u64,
    pub manager_fee: u64,
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorCommitIncentivesActionLog {
    pub spent_reward_slot: RewardDistributorRewardSlotValue,
    pub created_commitment_slot: RewardDistributorCommitmentSlotValue,
    pub created_reward_slots: Vec<RewardDistributorRewardSlotValue>,
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorInitiatePayoutActionLog {
    pub spent_entry_slot: RewardDistributorEntrySlotValue,
    pub created_entry_slot: RewardDistributorEntrySlotValue,
    pub entry_payout_amount: u64,
    pub payout_rounding_error: u128,
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorNewEpochActionLog {
    pub spent_reward_slot: RewardDistributorRewardSlotValue,
    pub created_reward_slot: RewardDistributorRewardSlotValue,
    pub epoch_total_rewards: u64,
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorSyncActionLog {
    pub update_time: u64,
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorWithdrawIncentivesActionLog {
    pub spent_reward_slot: RewardDistributorRewardSlotValue,
    pub spent_commitment_slot: RewardDistributorCommitmentSlotValue,
    pub created_reward_slot: RewardDistributorRewardSlotValue,
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorRefreshNftsFromDlActionLog {
    pub spent_entry_slots: Vec<RewardDistributorEntrySlotValue>,
    pub created_entry_slots: Vec<RewardDistributorEntrySlotValue>,
    #[serde(default)]
    pub nft_entries: Vec<RewardDistributorNftStakeEntry>,
    pub dl_root_hash: Bytes32,
    pub dl_inner_puzzle_hash: Bytes32,
    pub dl_full_puzzle_hash: Bytes32,
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorStakeActionLog {
    pub spent_entry_slot: Option<RewardDistributorEntrySlotValue>,
    pub created_entry_slot: RewardDistributorEntrySlotValue,
    pub cat_amount: Option<u64>,
    pub nft_entries: Option<Vec<RewardDistributorNftStakeEntry>>,
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RewardDistributorUnstakeActionLog {
    pub spent_entry_slot: RewardDistributorEntrySlotValue,
    pub created_entry_slot: RewardDistributorEntrySlotValue,
    pub cat_amount: Option<u64>,
    pub nft_entries: Option<Vec<RewardDistributorNftStakeEntry>>,
    pub changes: RewardDistributorStateTransition,
}

/// A parsed Reward Distributor action serialized as a stable `{"type": "Variant", "payload": {...}}` object.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum RewardDistributorActionLog {
    AddEntry(RewardDistributorAddEntryActionLog),
    RemoveEntry(RewardDistributorRemoveEntryActionLog),
    AddIncentives(RewardDistributorAddIncentivesActionLog),
    CommitIncentives(RewardDistributorCommitIncentivesActionLog),
    InitiatePayout(RewardDistributorInitiatePayoutActionLog),
    NewEpoch(RewardDistributorNewEpochActionLog),
    Sync(RewardDistributorSyncActionLog),
    WithdrawIncentives(RewardDistributorWithdrawIncentivesActionLog),
    RefreshNftsFromDl(RewardDistributorRefreshNftsFromDlActionLog),
    Stake(RewardDistributorStakeActionLog),
    Unstake(RewardDistributorUnstakeActionLog),
}

impl RewardDistributorActionLog {
    pub fn extend_spent_slots(
        &self,
        spent_reward_slots: &mut Vec<RewardDistributorRewardSlotValue>,
        spent_commitment_slots: &mut Vec<RewardDistributorCommitmentSlotValue>,
        spent_entry_slots: &mut Vec<RewardDistributorEntrySlotValue>,
    ) {
        match self {
            Self::AddEntry(_) | Self::AddIncentives(_) | Self::Sync(_) => {}
            Self::RemoveEntry(log) => spent_entry_slots.push(log.spent_entry_slot),
            Self::CommitIncentives(log) => spent_reward_slots.push(log.spent_reward_slot),
            Self::InitiatePayout(log) => spent_entry_slots.push(log.spent_entry_slot),
            Self::NewEpoch(log) => spent_reward_slots.push(log.spent_reward_slot),
            Self::WithdrawIncentives(log) => {
                spent_reward_slots.push(log.spent_reward_slot);
                spent_commitment_slots.push(log.spent_commitment_slot);
            }
            Self::RefreshNftsFromDl(log) => {
                spent_entry_slots.extend(log.spent_entry_slots.iter().copied());
            }
            Self::Stake(log) => {
                if let Some(spent_entry_slot) = log.spent_entry_slot {
                    spent_entry_slots.push(spent_entry_slot);
                }
            }
            Self::Unstake(log) => spent_entry_slots.push(log.spent_entry_slot),
        }
    }

    pub fn extend_created_slots(
        &self,
        created_reward_slots: &mut Vec<RewardDistributorRewardSlotValue>,
        created_commitment_slots: &mut Vec<RewardDistributorCommitmentSlotValue>,
        created_entry_slots: &mut Vec<RewardDistributorEntrySlotValue>,
    ) {
        match self {
            Self::AddEntry(log) => created_entry_slots.push(log.created_entry_slot),
            Self::RemoveEntry(_) | Self::AddIncentives(_) | Self::Sync(_) => {}
            Self::CommitIncentives(log) => {
                created_commitment_slots.push(log.created_commitment_slot);
                created_reward_slots.extend(log.created_reward_slots.iter().copied());
            }
            Self::InitiatePayout(log) => created_entry_slots.push(log.created_entry_slot),
            Self::NewEpoch(log) => created_reward_slots.push(log.created_reward_slot),
            Self::WithdrawIncentives(log) => created_reward_slots.push(log.created_reward_slot),
            Self::RefreshNftsFromDl(log) => {
                created_entry_slots.extend(log.created_entry_slots.iter().copied());
            }
            Self::Stake(log) => created_entry_slots.push(log.created_entry_slot),
            Self::Unstake(log) => created_entry_slots.push(log.created_entry_slot),
        }
    }
}
