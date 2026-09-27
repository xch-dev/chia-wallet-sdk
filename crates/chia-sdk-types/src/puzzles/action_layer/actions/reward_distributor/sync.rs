use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_SYNC_PUZZLE: [u8; 249] = hex!(
    // Rue
    "
    ff02ffff03ffff22ffff20ffff15ff03ff7e8080ffff15ff03ff5e8080ffff01
    ff02ffff01ff04ffff04ff09ffff04ff15ffff04ff2dffff04ffff04ffff10ff
    819dff0280ffff11ff81ddffff12ff02ff2d808080ffff04ff07ff81fd808080
    8080ffff04ffff04ffff0151ffff04ff07ff808080ffff04ffff04ffff013eff
    ff04ffff0effff0173ffff0bffff0102ffff0bffff0101ff0780ffff0bffff01
    01ff81fd808080ff808080ff80808080ffff04ffff02ffff03ffff15ff16ff80
    80ffff01ff13ffff12ff6effff11ff03ff5e8080ffff12ff16ffff11ff7eff5e
    808080ffff018080ff0180ff018080ffff01ff088080ff0180
    "
);

pub const REWARD_DISTRIBUTOR_SYNC_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    1a4d3e443be05a124980741db509657d5b49a0405d9646179e8a498ae2fe4343
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
#[clvm(transparent)]
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
