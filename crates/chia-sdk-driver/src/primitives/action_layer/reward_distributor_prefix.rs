use chia_protocol::Bytes32;
use clvm_traits::clvm_tuple;
use clvm_utils::{ToTreeHash, TreeHash};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RewardDistributorCreatedAnnouncementPrefix {
    AddIncentives = b'i',
    CommitIncentives = b'c',
    InitiatePayout = b'p',
    NewEpoch = b'e',
    Sync = b's',
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RewardDistributorReceivedMessagePrefix {
    InitiatePayout = b'p',
    WithdrawIncentives = b'w',
    AddEntry = b'a',
    RemoveEntry = b'r',
}

pub fn prefix_hash(prefix: u8, hash: TreeHash) -> Vec<u8> {
    let mut msg = hash.to_vec();
    msg.insert(0, prefix);
    msg
}

impl RewardDistributorCreatedAnnouncementPrefix {
    pub fn add_incentives(amount: u64, epoch_end: u64) -> Vec<u8> {
        prefix_hash(
            Self::AddIncentives as u8,
            clvm_tuple!(amount, epoch_end).tree_hash(),
        )
    }

    pub fn commit_incentives(new_commitment_slot_value: TreeHash) -> Vec<u8> {
        prefix_hash(Self::CommitIncentives as u8, new_commitment_slot_value)
    }

    pub fn initiate_payout(payout_puzzle_hash: Bytes32, payout_amount: u64) -> Vec<u8> {
        prefix_hash(
            Self::InitiatePayout as u8,
            clvm_tuple!(payout_puzzle_hash, payout_amount).tree_hash(),
        )
    }

    pub fn new_epoch(epoch_end: u64) -> Vec<u8> {
        prefix_hash(Self::NewEpoch as u8, epoch_end.tree_hash())
    }

    pub fn sync(update_time: u64, epoch_end: u64) -> Vec<u8> {
        prefix_hash(
            Self::Sync as u8,
            clvm_tuple!(update_time, epoch_end).tree_hash(),
        )
    }
}

impl RewardDistributorReceivedMessagePrefix {
    pub fn initiate_payout(payout_amount: u64, payout_rounding_error: u128) -> Vec<u8> {
        prefix_hash(
            Self::InitiatePayout as u8,
            clvm_tuple!(payout_amount, payout_rounding_error).tree_hash(),
        )
    }

    pub fn withdraw_incentives(reward_slot_epoch_time: u64, committed_value: u64) -> Vec<u8> {
        prefix_hash(
            Self::WithdrawIncentives as u8,
            clvm_tuple!(reward_slot_epoch_time, committed_value).tree_hash(),
        )
    }

    pub fn add_entry(payout_puzzle_hash: Bytes32, shares: u64) -> Vec<u8> {
        prefix_hash(
            Self::AddEntry as u8,
            clvm_tuple!(payout_puzzle_hash, shares).tree_hash(),
        )
    }

    pub fn remove_entry(payout_puzzle_hash: Bytes32, shares: u64) -> Vec<u8> {
        prefix_hash(
            Self::RemoveEntry as u8,
            clvm_tuple!(payout_puzzle_hash, shares).tree_hash(),
        )
    }
}
