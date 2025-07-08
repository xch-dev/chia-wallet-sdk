use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_SYNC_PUZZLE: [u8; 308] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff20ffff15ff13ff8201bd8080ffff15ff13
    ff82013d8080ffff01ff04ffff04ff09ffff04ff15ffff04ff2dffff04ffff02
    ff0effff04ff02ffff04ff2dffff04ff819dffff04ff81ddffff04ffff02ffff
    03ffff15ff2dff8080ffff01ff05ffff14ffff12ff81ddffff11ff13ff82013d
    8080ffff12ff2dffff11ff8201bdff82013d80808080ff8080ff0180ff808080
    80808080ffff04ffff04ff13ff8201bd80ff808080808080ffff04ffff04ff04
    ffff04ff13ff808080ffff04ffff04ff0affff04ffff0effff0173ffff0bffff
    0102ffff0bffff0101ff1380ffff0bffff0101ff8201bd808080ff808080ff80
    808080ffff01ff088080ff0180ffff04ffff01ff51ff3eff04ffff10ff0bff2f
    80ffff11ff17ffff12ff2fff05808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_SYNC_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    9e2707ff8a4f5b52feb763a80c5c23073e588172c6220b4146f72b484c064546
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
