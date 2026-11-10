use chia_sdk_types::{
    Conditions, announcement_id,
    puzzles::{RewardDistributorSyncActionArgs, RewardDistributorSyncActionSolution},
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, RewardDistributor, RewardDistributorConstants,
    RewardDistributorCreatedAnnouncementPrefix, RewardDistributorStateTransition,
    RewardDistributorSyncActionLog, SingletonAction, Spend, SpendContext,
};

/// The reward distributor action that distributes the current epoch's rewards up to a given
/// timestamp.
///
/// Anyone can run it to move the distributor's last update time forward, which updates the
/// cumulative payout for the time that passed.
///
/// Use [`SingletonAction::from_constants`] to create it from a distributor's constants.
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

    /// Parses an action solution into a [`RewardDistributorSyncActionLog`].
    ///
    /// `changes` is the distributor state before and after the action.
    ///
    /// # Errors
    ///
    /// Returns an error if the solution can't be parsed.
    pub fn get_log(
        ctx: &SpendContext,
        solution: NodePtr,
        changes: RewardDistributorStateTransition,
    ) -> Result<RewardDistributorSyncActionLog, DriverError> {
        let solution = ctx.extract::<RewardDistributorSyncActionSolution>(solution)?;

        Ok(RewardDistributorSyncActionLog {
            update_time: solution.update_time,
            changes,
        })
    }

    /// Adds a sync to `update_time` to the pending spend of `distributor`.
    ///
    /// Returns a condition that asserts the distributor's sync announcement for `update_time`
    /// and the current epoch end.
    ///
    /// # Errors
    ///
    /// Returns an error if the action spend can't be created.
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        update_time: u64,
    ) -> Result<Conditions, DriverError> {
        // calculate announcement needed to ensure everything's happening as expected
        let my_state = distributor.pending_spend.latest_state.1;
        let new_epoch_announcement = RewardDistributorCreatedAnnouncementPrefix::sync(
            update_time,
            my_state.round_time_info.epoch_end,
        );
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
