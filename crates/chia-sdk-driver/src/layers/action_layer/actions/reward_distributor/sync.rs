use chia_sdk_types::{
    announcement_id,
    puzzles::{RewardDistributorSyncActionArgs, RewardDistributorSyncActionSolution},
    Conditions,
};
use clvm_traits::clvm_tuple;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, RewardDistributor, RewardDistributorConstants, SingletonAction, Spend,
    SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorSyncAction {}

impl ToTreeHash for RewardDistributorSyncAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorSyncActionArgs::curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorSyncAction {
    fn from_constants(_constants: &RewardDistributorConstants) -> Self {
        Self {}
    }
}

impl RewardDistributorSyncAction {
    fn construct_puzzle(ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.alloc_mod::<RewardDistributorSyncActionArgs>()
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        update_time: u64,
    ) -> Result<Conditions, DriverError> {
        // calculate announcement needed to ensure everything's happening as expected
        let my_state = distributor.pending_spend.latest_state.1;
        let mut new_epoch_announcement =
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
        let action_puzzle = Self::construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok(new_epoch_conditions)
    }
}
