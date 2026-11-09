use chia_protocol::Bytes32;
use clvm_traits::clvm_tuple;
use clvm_utils::{ToTreeHash, TreeHash};

/// Prefixes of the puzzle announcements that the [`RewardDistributor`](crate::RewardDistributor) creates.
///
/// Each announcement is one prefix byte followed by a 32-byte hash. Coins involved in an action
/// assert these announcements to make sure the action happened as expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RewardDistributorCreatedAnnouncementPrefix {
    /// Announced by the add incentives action (`i`).
    AddIncentives = b'i',
    /// Announced by the commit incentives action (`c`).
    CommitIncentives = b'c',
    /// Announced by the initiate payout action when payouts don't require approval (`p`).
    InitiatePayout = b'p',
    /// Announced by the new epoch action (`e`).
    NewEpoch = b'e',
    /// Announced by the sync action (`s`).
    Sync = b's',
    /// Announced by the stake action for the entry slot it creates (`t`).
    StakeSlot = b't',
    /// Announced by the stake action for each asset it locks (`l`).
    StakeLock = b'l',
    /// Announced by the refresh action for each NFT it refreshes (`r`).
    Refresh = b'r',
}

/// Prefixes of the messages that the [`RewardDistributor`](crate::RewardDistributor) receives.
///
/// Each message is one prefix byte followed by a 32-byte hash. They are sent by the party that
/// must approve an action, such as the manager singleton or the custody puzzle of an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RewardDistributorReceivedMessagePrefix {
    /// Approves a payout when payouts require approval (`p`).
    InitiatePayout = b'p',
    /// Sent by a commitment's clawback puzzle to withdraw committed incentives (`w`).
    WithdrawIncentives = b'w',
    /// Sent by the manager singleton to add an entry (`a`).
    AddEntry = b'a',
    /// Sent by the manager singleton to remove an entry (`r`).
    RemoveEntry = b'r',
    /// Sent when staking more into an existing entry, giving up its unclaimed rewards (`s`).
    Stake = b's',
    /// Sent by the custody puzzle of an entry to unstake a locked NFT or CAT (`u`).
    Unstake = b'u',
}

/// Returns `prefix` followed by the 32 bytes of `hash`.
///
/// This is the format of every action-layer announcement and message in this module.
///
/// ```
/// use chia_sdk_driver::prefix_hash;
/// use clvm_utils::TreeHash;
///
/// let message = prefix_hash(b'x', TreeHash::new([7; 32]));
/// assert_eq!(message[0], b'x');
/// assert_eq!(&message[1..], &[7; 32]);
/// ```
pub fn prefix_hash(prefix: u8, hash: TreeHash) -> Vec<u8> {
    let mut msg = hash.to_vec();
    msg.insert(0, prefix);
    msg
}

impl RewardDistributorCreatedAnnouncementPrefix {
    /// Returns the announcement for adding `amount` incentives when the state was last updated at `epoch_last_update`.
    pub fn add_incentives(amount: u64, epoch_last_update: u64) -> Vec<u8> {
        prefix_hash(
            Self::AddIncentives as u8,
            clvm_tuple!(amount, epoch_last_update).tree_hash(),
        )
    }

    /// Returns the announcement for a commitment, given the tree hash of the commitment slot value it creates.
    pub fn commit_incentives(new_commitment_slot_value: TreeHash) -> Vec<u8> {
        prefix_hash(Self::CommitIncentives as u8, new_commitment_slot_value)
    }

    /// Returns the announcement for paying `payout_amount` to `payout_puzzle_hash`.
    pub fn initiate_payout(payout_puzzle_hash: Bytes32, payout_amount: u64) -> Vec<u8> {
        prefix_hash(
            Self::InitiatePayout as u8,
            clvm_tuple!(payout_puzzle_hash, payout_amount).tree_hash(),
        )
    }

    /// Returns the announcement for starting the epoch that begins at `epoch_end` (the end of the previous one).
    pub fn new_epoch(epoch_end: u64) -> Vec<u8> {
        prefix_hash(Self::NewEpoch as u8, epoch_end.tree_hash())
    }

    /// Returns the announcement for syncing the distributor to `update_time` within the epoch ending at `epoch_end`.
    pub fn sync(update_time: u64, epoch_end: u64) -> Vec<u8> {
        prefix_hash(
            Self::Sync as u8,
            clvm_tuple!(update_time, epoch_end).tree_hash(),
        )
    }

    /// Returns the announcement for a stake, given the tree hash of the entry slot value it creates.
    pub fn stake_slot(new_entry_slot_value_hash: TreeHash) -> Vec<u8> {
        prefix_hash(Self::StakeSlot as u8, new_entry_slot_value_hash)
    }

    /// Returns the announcement for locking an asset, given the id of the offer announcement that pays it to the distributor.
    pub fn stake_lock(offer_announcement_id: Bytes32) -> Vec<u8> {
        prefix_hash(Self::StakeLock as u8, offer_announcement_id.into())
    }

    /// Returns the announcement for refreshing the staked NFT with the given launcher id.
    pub fn refresh(nft_launcher_id: Bytes32) -> Vec<u8> {
        prefix_hash(Self::Refresh as u8, nft_launcher_id.into())
    }
}

impl RewardDistributorReceivedMessagePrefix {
    /// Returns the approval message for paying out `payout_amount` with the given rounding error.
    pub fn initiate_payout(payout_amount: u64, payout_rounding_error: u128) -> Vec<u8> {
        prefix_hash(
            Self::InitiatePayout as u8,
            clvm_tuple!(payout_amount, payout_rounding_error).tree_hash(),
        )
    }

    /// Returns the message for withdrawing a commitment of `committed_value` from the epoch starting at `reward_slot_epoch_time`.
    pub fn withdraw_incentives(reward_slot_epoch_time: u64, committed_value: u64) -> Vec<u8> {
        prefix_hash(
            Self::WithdrawIncentives as u8,
            clvm_tuple!(reward_slot_epoch_time, committed_value).tree_hash(),
        )
    }

    /// Returns the message for adding an entry with `shares` shares paid to `payout_puzzle_hash`.
    pub fn add_entry(payout_puzzle_hash: Bytes32, shares: u64) -> Vec<u8> {
        prefix_hash(
            Self::AddEntry as u8,
            clvm_tuple!(payout_puzzle_hash, shares).tree_hash(),
        )
    }

    /// Returns the message for removing the entry with `shares` shares paid to `payout_puzzle_hash`.
    pub fn remove_entry(payout_puzzle_hash: Bytes32, shares: u64) -> Vec<u8> {
        prefix_hash(
            Self::RemoveEntry as u8,
            clvm_tuple!(payout_puzzle_hash, shares).tree_hash(),
        )
    }

    /// Returns the message for staking into an existing entry, giving up `rewards_to_give_up` (with precision) of unclaimed rewards.
    pub fn stake(rewards_to_give_up: u128) -> Vec<u8> {
        prefix_hash(Self::Stake as u8, rewards_to_give_up.tree_hash())
    }

    /// Returns the message for unstaking the NFT with the given launcher id.
    pub fn unstake_nft(nft_launcher_id: Bytes32) -> Vec<u8> {
        prefix_hash(Self::Unstake as u8, nft_launcher_id.into())
    }

    /// Returns the message for unstaking the locked CAT with the given parent coin id.
    pub fn unstake_cat(cat_parent_id: Bytes32) -> Vec<u8> {
        prefix_hash(Self::Unstake as u8, cat_parent_id.into())
    }
}
