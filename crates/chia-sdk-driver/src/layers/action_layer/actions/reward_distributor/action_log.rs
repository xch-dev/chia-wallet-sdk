use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::{
    RewardDistributorCommitmentSlotValue, RewardDistributorDepositSlotValue,
    RewardDistributorEntrySlotValue, RewardDistributorRewardSlotValue,
};

use crate::RewardDistributorState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// An NFT staked in, unstaked from, or refreshed in a reward distributor.
pub struct RewardDistributorNftStakeEntry {
    /// The launcher id of the NFT.
    pub launcher_id: Bytes32,
    /// The shares of the NFT. For a refresh, this is the NFT's new number of shares.
    pub shares: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// The state of a reward distributor before and after an action.
pub struct RewardDistributorStateTransition {
    /// The state before the action.
    pub old_state: RewardDistributorState,
    /// The state after the action.
    pub new_state: RewardDistributorState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// What a [`RewardDistributorAddEntryAction`](crate::RewardDistributorAddEntryAction) spend did.
pub struct RewardDistributorAddEntryActionLog {
    /// The new entry slot.
    pub created_entry_slot: RewardDistributorEntrySlotValue,
    /// The inner puzzle hash of the manager singleton that approved the action.
    pub manager_singleton_inner_puzzle_hash: Bytes32,
    /// The distributor state before and after the action.
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// What a [`RewardDistributorRemoveEntryAction`](crate::RewardDistributorRemoveEntryAction) spend
/// did.
pub struct RewardDistributorRemoveEntryActionLog {
    /// The removed entry slot.
    pub spent_entry_slot: RewardDistributorEntrySlotValue,
    /// The inner puzzle hash of the manager singleton that approved the action.
    pub manager_singleton_inner_puzzle_hash: Bytes32,
    /// The distributor state before and after the action.
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// What a [`RewardDistributorAddIncentivesAction`](crate::RewardDistributorAddIncentivesAction)
/// spend did.
pub struct RewardDistributorAddIncentivesActionLog {
    /// The amount of incentives added.
    pub amount: u64,
    /// The fee charged on [`amount`](Self::amount), which is `amount * fee_bps / 10_000`.
    pub manager_fee: u64,
    /// The distributor state before and after the action.
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// What a [`RewardDistributorCommitIncentivesAction`](crate::RewardDistributorCommitIncentivesAction)
/// spend did.
pub struct RewardDistributorCommitIncentivesActionLog {
    /// The reward slot that was spent.
    pub spent_reward_slot: RewardDistributorRewardSlotValue,
    /// The new commitment slot, which records who committed the rewards so they can be
    /// withdrawn later.
    pub created_commitment_slot: RewardDistributorCommitmentSlotValue,
    /// The created reward slots.
    ///
    /// If the rewards are for the epoch of the spent slot, this is that slot with the rewards
    /// added. Otherwise, it's the spent slot marked as having a next epoch, a slot for the
    /// committed epoch, and empty slots for the epochs in between.
    pub created_reward_slots: Vec<RewardDistributorRewardSlotValue>,
    /// The distributor state before and after the action.
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// What a [`RewardDistributorInitiatePayoutAction`](crate::RewardDistributorInitiatePayoutAction)
/// spend did.
pub struct RewardDistributorInitiatePayoutActionLog {
    /// The entry slot before the payout.
    pub spent_entry_slot: RewardDistributorEntrySlotValue,
    /// The entry slot after the payout, whose initial cumulative payout is the current one.
    pub created_entry_slot: RewardDistributorEntrySlotValue,
    /// The amount paid to the entry's payout puzzle hash.
    pub entry_payout_amount: u64,
    /// The remainder lost when dividing the payout by the distributor's precision.
    pub payout_rounding_error: u128,
    /// The distributor state before and after the action.
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// What a [`RewardDistributorNewEpochAction`](crate::RewardDistributorNewEpochAction) spend did.
pub struct RewardDistributorNewEpochActionLog {
    /// The reward slot that was spent to get the new epoch's rewards.
    pub spent_reward_slot: RewardDistributorRewardSlotValue,
    /// The recreated reward slot, which only has its counter incremented.
    pub created_reward_slot: RewardDistributorRewardSlotValue,
    /// The total rewards of the new epoch, before the fee. It's 0 if the spent slot isn't for
    /// the new epoch.
    pub epoch_total_rewards: u64,
    /// The distributor state before and after the action.
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// What a [`RewardDistributorSyncAction`](crate::RewardDistributorSyncAction) spend did.
pub struct RewardDistributorSyncActionLog {
    /// The timestamp the distributor was synced to.
    pub update_time: u64,
    /// The distributor state before and after the action.
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// What a [`RewardDistributorWithdrawIncentivesAction`](crate::RewardDistributorWithdrawIncentivesAction)
/// spend did.
pub struct RewardDistributorWithdrawIncentivesActionLog {
    /// The reward slot of the epoch the rewards were committed to, before the withdrawal.
    pub spent_reward_slot: RewardDistributorRewardSlotValue,
    /// The commitment slot that was withdrawn from.
    pub spent_commitment_slot: RewardDistributorCommitmentSlotValue,
    /// The reward slot after the withdrawal, with the withdrawn share of the rewards removed.
    pub created_reward_slot: RewardDistributorRewardSlotValue,
    /// The distributor state before and after the action.
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// What a [`RewardDistributorRefreshAction`](crate::RewardDistributorRefreshAction) spend did.
pub struct RewardDistributorRefreshNftsFromDlActionLog {
    /// The entry slots that were spent, one per refreshed entry.
    pub spent_entry_slots: Vec<RewardDistributorEntrySlotValue>,
    /// The recreated entry slots, with updated shares and the current cumulative payout.
    pub created_entry_slots: Vec<RewardDistributorEntrySlotValue>,
    /// The deposit slots of the refreshed NFTs, with their old shares.
    #[serde(default)]
    pub spent_deposit_slots: Vec<RewardDistributorDepositSlotValue>,
    /// The recreated deposit slots of the refreshed NFTs, with their new shares.
    #[serde(default)]
    pub created_deposit_slots: Vec<RewardDistributorDepositSlotValue>,
    /// The refreshed NFTs and their new shares.
    #[serde(default)]
    pub nft_entries: Vec<RewardDistributorNftStakeEntry>,
    /// The root hash of the curated `DataLayer` store that lists the NFT shares.
    pub dl_root_hash: Bytes32,
    /// The inner puzzle hash of the `DataLayer` store singleton.
    pub dl_inner_puzzle_hash: Bytes32,
    /// The full puzzle hash of the `DataLayer` store singleton.
    pub dl_full_puzzle_hash: Bytes32,
    /// The distributor state before and after the action.
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// What a [`RewardDistributorStakeAction`](crate::RewardDistributorStakeAction) spend did.
pub struct RewardDistributorStakeActionLog {
    /// The existing entry slot of the custody puzzle hash, or `None` if a new entry was
    /// created.
    pub spent_entry_slot: Option<RewardDistributorEntrySlotValue>,
    /// The entry slot after the stake, with the new shares added.
    pub created_entry_slot: RewardDistributorEntrySlotValue,
    /// The new deposit slots, one per staked NFT or one for the staked CAT.
    #[serde(default)]
    pub created_deposit_slots: Vec<RewardDistributorDepositSlotValue>,
    /// The staked CAT amount, for a [`RewardDistributorType::Cat`](crate::RewardDistributorType::Cat)
    /// distributor.
    pub cat_amount: Option<u64>,
    /// The staked NFTs, for an NFT distributor.
    pub nft_entries: Option<Vec<RewardDistributorNftStakeEntry>>,
    /// The distributor state before and after the action.
    pub changes: RewardDistributorStateTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// What a [`RewardDistributorUnstakeAction`](crate::RewardDistributorUnstakeAction) spend did.
pub struct RewardDistributorUnstakeActionLog {
    /// The entry slot before the unstake.
    pub spent_entry_slot: RewardDistributorEntrySlotValue,
    /// The entry slot after the unstake, with the unstaked shares removed.
    pub created_entry_slot: RewardDistributorEntrySlotValue,
    /// The deposit slots of the unstaked NFTs or CAT.
    #[serde(default)]
    pub spent_deposit_slots: Vec<RewardDistributorDepositSlotValue>,
    /// The unstaked CAT amount, for a [`RewardDistributorType::Cat`](crate::RewardDistributorType::Cat)
    /// distributor.
    pub cat_amount: Option<u64>,
    /// The unstaked NFTs, for an NFT distributor.
    pub nft_entries: Option<Vec<RewardDistributorNftStakeEntry>>,
    /// The distributor state before and after the action.
    pub changes: RewardDistributorStateTransition,
}

/// A parsed action from a reward distributor spend.
///
/// It's serialized with serde as a stable `{"type": "Variant", "payload": {...}}` object.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum RewardDistributorActionLog {
    /// A [`RewardDistributorAddEntryAction`](crate::RewardDistributorAddEntryAction).
    AddEntry(RewardDistributorAddEntryActionLog),
    /// A [`RewardDistributorRemoveEntryAction`](crate::RewardDistributorRemoveEntryAction).
    RemoveEntry(RewardDistributorRemoveEntryActionLog),
    /// A [`RewardDistributorAddIncentivesAction`](crate::RewardDistributorAddIncentivesAction).
    AddIncentives(RewardDistributorAddIncentivesActionLog),
    /// A [`RewardDistributorCommitIncentivesAction`](crate::RewardDistributorCommitIncentivesAction).
    CommitIncentives(RewardDistributorCommitIncentivesActionLog),
    /// A [`RewardDistributorInitiatePayoutAction`](crate::RewardDistributorInitiatePayoutAction).
    InitiatePayout(RewardDistributorInitiatePayoutActionLog),
    /// A [`RewardDistributorNewEpochAction`](crate::RewardDistributorNewEpochAction).
    NewEpoch(RewardDistributorNewEpochActionLog),
    /// A [`RewardDistributorSyncAction`](crate::RewardDistributorSyncAction).
    Sync(RewardDistributorSyncActionLog),
    /// A [`RewardDistributorWithdrawIncentivesAction`](crate::RewardDistributorWithdrawIncentivesAction).
    WithdrawIncentives(RewardDistributorWithdrawIncentivesActionLog),
    /// A [`RewardDistributorRefreshAction`](crate::RewardDistributorRefreshAction).
    RefreshNftsFromDl(RewardDistributorRefreshNftsFromDlActionLog),
    /// A [`RewardDistributorStakeAction`](crate::RewardDistributorStakeAction).
    Stake(RewardDistributorStakeActionLog),
    /// A [`RewardDistributorUnstakeAction`](crate::RewardDistributorUnstakeAction).
    Unstake(RewardDistributorUnstakeActionLog),
}

impl RewardDistributorActionLog {
    /// Appends the slots spent by this action to the vector for each slot type.
    pub fn extend_spent_slots(
        &self,
        spent_reward_slots: &mut Vec<RewardDistributorRewardSlotValue>,
        spent_commitment_slots: &mut Vec<RewardDistributorCommitmentSlotValue>,
        spent_entry_slots: &mut Vec<RewardDistributorEntrySlotValue>,
        spent_deposit_slots: &mut Vec<RewardDistributorDepositSlotValue>,
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
                spent_deposit_slots.extend(log.spent_deposit_slots.iter().copied());
            }
            Self::Stake(log) => {
                if let Some(spent_entry_slot) = log.spent_entry_slot {
                    spent_entry_slots.push(spent_entry_slot);
                }
            }
            Self::Unstake(log) => {
                spent_entry_slots.push(log.spent_entry_slot);
                spent_deposit_slots.extend(log.spent_deposit_slots.iter().copied());
            }
        }
    }

    /// Appends the slots created by this action to the vector for each slot type.
    pub fn extend_created_slots(
        &self,
        created_reward_slots: &mut Vec<RewardDistributorRewardSlotValue>,
        created_commitment_slots: &mut Vec<RewardDistributorCommitmentSlotValue>,
        created_entry_slots: &mut Vec<RewardDistributorEntrySlotValue>,
        created_deposit_slots: &mut Vec<RewardDistributorDepositSlotValue>,
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
                created_deposit_slots.extend(log.created_deposit_slots.iter().copied());
            }
            Self::Stake(log) => {
                created_entry_slots.push(log.created_entry_slot);
                created_deposit_slots.extend(log.created_deposit_slots.iter().copied());
            }
            Self::Unstake(log) => created_entry_slots.push(log.created_entry_slot),
        }
    }
}
