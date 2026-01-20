use clvm_traits::clvm_tuple;
use clvm_utils::{ToTreeHash, TreeHash};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RewardDistributorPrefix {
    AddIncentivesCreatedPuzzleAnnouncement = b'i',
    CommitIncentivesCreatedPuzzleAnnouncement = b'c',
}

impl RewardDistributorPrefix {
    pub fn prefix_hash(&self, hash: TreeHash) -> Vec<u8> {
        let mut msg = hash.to_vec();
        msg.insert(0, *self as u8);
        msg
    }

    pub fn add_incentives_announcement(amount: u64, epoch_end: u64) -> Vec<u8> {
        Self::AddIncentivesCreatedPuzzleAnnouncement
            .prefix_hash(clvm_tuple!(amount, epoch_end).tree_hash())
    }
}
