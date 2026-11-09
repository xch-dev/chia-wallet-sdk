use chia_protocol::Bytes32;
use chia_sdk_types::{
    Conditions, Mod, announcement_id,
    puzzles::{
        RewardDistributorAddIncentivesActionArgs, RewardDistributorAddIncentivesActionSolution,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, RewardDistributor, RewardDistributorAddIncentivesActionLog,
    RewardDistributorConstants, RewardDistributorCreatedAnnouncementPrefix,
    RewardDistributorStateTransition, SingletonAction, Spend, SpendContext,
};

/// The reward distributor action that adds incentives to the current epoch.
///
/// A fee of [`fee_bps`](Self::fee_bps) basis points is taken from the added amount and paid
/// to [`fee_payout_puzzle_hash`](Self::fee_payout_puzzle_hash).
///
/// Use [`SingletonAction::from_constants`] to create it from a distributor's constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorAddIncentivesAction {
    /// See [`RewardDistributorConstants::fee_payout_puzzle_hash`](crate::RewardDistributorConstants::fee_payout_puzzle_hash).
    pub fee_payout_puzzle_hash: Bytes32,
    /// See [`RewardDistributorConstants::fee_bps`](crate::RewardDistributorConstants::fee_bps).
    pub fee_bps: u64,
    /// See [`RewardDistributorConstants::precision`](crate::RewardDistributorConstants::precision).
    pub precision: u64,
}

impl ToTreeHash for RewardDistributorAddIncentivesAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorAddIncentivesActionArgs {
            fee_payout_puzzle_hash: self.fee_payout_puzzle_hash,
            fee_bps: self.fee_bps,
            precision: self.precision,
        }
        .curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorAddIncentivesAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            fee_payout_puzzle_hash: constants.fee_payout_puzzle_hash,
            fee_bps: constants.fee_bps,
            precision: constants.precision,
        }
    }
}

impl RewardDistributorAddIncentivesAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(RewardDistributorAddIncentivesActionArgs {
            fee_payout_puzzle_hash: self.fee_payout_puzzle_hash,
            fee_bps: self.fee_bps,
            precision: self.precision,
        })
    }

    /// Parses an action solution into a [`RewardDistributorAddIncentivesActionLog`].
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
    ) -> Result<RewardDistributorAddIncentivesActionLog, DriverError> {
        let solution = ctx.extract::<RewardDistributorAddIncentivesActionSolution>(solution)?;

        Ok(RewardDistributorAddIncentivesActionLog {
            amount: solution.amount,
            manager_fee: solution.manager_fee,
            changes,
        })
    }

    /// Adds `amount` of incentives in the pending spend of `distributor`.
    ///
    /// Returns a condition that asserts the distributor's add incentives announcement.
    ///
    /// # Errors
    ///
    /// Returns an error if the action spend can't be created.
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        amount: u64,
    ) -> Result<Conditions, DriverError> {
        let my_state = distributor.pending_spend.latest_state.1;

        // calculate announcement needed to ensure everything's happening as expected
        let add_incentives_announcement =
            Conditions::new().assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                RewardDistributorCreatedAnnouncementPrefix::add_incentives(
                    amount,
                    my_state.round_time_info.last_update,
                ),
            ));

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorAddIncentivesActionSolution {
            amount,
            manager_fee: amount * distributor.info.constants.fee_bps / 10000,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok(add_incentives_announcement)
    }
}
