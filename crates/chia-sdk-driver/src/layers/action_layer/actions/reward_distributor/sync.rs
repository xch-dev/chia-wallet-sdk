use chia::clvm_utils::{ToTreeHash, TreeHash};
use chia_wallet_sdk::{
    driver::{DriverError, Spend, SpendContext},
    types::{announcement_id, Conditions},
};
use clvm_traits::{clvm_tuple, FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{Action, RewardDistributor, RewardDistributorConstants, SpendContextExt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardDistributorSyncAction {}

impl ToTreeHash for RewardDistributorSyncAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorSyncActionArgs::curry_tree_hash()
    }
}

impl Action<RewardDistributor> for RewardDistributorSyncAction {
    fn from_constants(_constants: &RewardDistributorConstants) -> Self {
        Self {}
    }
}

impl RewardDistributorSyncAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.reward_distributor_sync_action_puzzle()
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        update_time: u64,
    ) -> Result<Conditions, DriverError> {
        // calculate announcement needed to ensure everything's happening as expected
        let my_state = distributor.pending_spend.latest_state.1;
        let mut new_epoch_announcement: Vec<u8> =
            clvm_tuple!(update_time, my_state.round_time_info.epoch_end)
                .tree_hash()
                .to_vec();
        new_epoch_announcement.insert(0, b's');
        let new_epoch_conditions = Conditions::new().assert_puzzle_announcement(announcement_id(
            distributor.coin.puzzle_hash,
            new_epoch_announcement,
        ));

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorSyncActionSolution { update_time })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok(new_epoch_conditions)
    }
}

pub const REWARD_DISTRIBUTOR_SYNC_PUZZLE: [u8; 308] = hex!("ff02ffff01ff02ffff03ffff22ffff20ffff15ff13ff8201bd8080ffff15ff13ff82013d8080ffff01ff04ffff04ff09ffff04ff15ffff04ff2dffff04ffff02ff0effff04ff02ffff04ff2dffff04ff819dffff04ff81ddffff04ffff02ffff03ffff15ff2dff8080ffff01ff05ffff14ffff12ff81ddffff11ff13ff82013d8080ffff12ff2dffff11ff8201bdff82013d80808080ff8080ff0180ff80808080808080ffff04ffff04ff13ff8201bd80ff808080808080ffff04ffff04ff04ffff04ff13ff808080ffff04ffff04ff0affff04ffff0effff0173ffff0bffff0102ffff0bffff0101ff1380ffff0bffff0101ff8201bd808080ff808080ff80808080ffff01ff088080ff0180ffff04ffff01ff51ff3eff04ffff10ff0bff2f80ffff11ff17ffff12ff2fff05808080ff018080");

pub const REWARD_DISTRIBUTOR_SYNC_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    9e2707ff8a4f5b52feb763a80c5c23073e588172c6220b4146f72b484c064546
    "
));

pub struct RewardDistributorSyncActionArgs {}
impl RewardDistributorSyncActionArgs {
    pub fn curry_tree_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_SYNC_PUZZLE_HASH
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorSyncActionSolution {
    pub update_time: u64,
}
