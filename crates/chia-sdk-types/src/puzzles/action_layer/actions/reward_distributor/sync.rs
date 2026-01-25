use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_SYNC_PUZZLE: [u8; 295] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff20ffff15ff0bff81fd8080ffff15ff0bff
    81bd8080ffff01ff04ffff04ff09ffff04ff15ffff04ff2dffff04ffff02ff0e
    ffff04ff02ffff04ff2dffff04ff819dffff04ff81ddffff04ffff02ffff03ff
    ff15ff2dff8080ffff01ff05ffff14ffff12ff81ddffff11ff0bff81bd8080ff
    ff12ff2dffff11ff81fdff81bd80808080ff8080ff0180ff80808080808080ff
    ff04ff0bff81fd8080808080ffff04ffff04ff04ffff04ff0bff808080ffff04
    ffff04ff0affff04ffff0effff0173ffff0bffff0102ffff0bffff0101ff0b80
    ffff0bffff0101ff81fd808080ff808080ff80808080ffff01ff088080ff0180
    ffff04ffff01ff51ff3eff04ffff10ff0bff2f80ffff11ff17ffff12ff2fff05
    808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_SYNC_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    8df5ceda958718d75f83be7f522365c7a6a1c6b8a7147a004faa536e55f2e7b9
    "
));

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RewardDistributorSyncActionArgs {}

impl RewardDistributorSyncActionArgs {
    pub fn curry_tree_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_SYNC_PUZZLE_HASH
    }
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorSyncActionSolution {
    pub update_time: u64,
}

impl Mod for RewardDistributorSyncActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_SYNC_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_SYNC_PUZZLE_HASH
    }
}
