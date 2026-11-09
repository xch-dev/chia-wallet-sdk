use chia_protocol::Bytes32;
use chia_sdk_types::{
    Conditions, Mod, announcement_id,
    puzzles::{
        RewardDistributorNewEpochActionArgs, RewardDistributorNewEpochActionSolution,
        RewardDistributorRewardSlotValue, RewardDistributorSlotNonce,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, RewardDistributor, RewardDistributorConstants,
    RewardDistributorCreatedAnnouncementPrefix, RewardDistributorNewEpochActionLog,
    RewardDistributorStateTransition, SingletonAction, Slot, Spend, SpendContext,
};

/// The reward distributor action that starts the next epoch.
///
/// It reads the next epoch's rewards from its reward slot and takes a fee of
/// [`fee_bps`](Self::fee_bps) basis points from them.
///
/// Use [`SingletonAction::from_constants`] to create it from a distributor's constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorNewEpochAction {
    /// The launcher id of the reward distributor.
    pub launcher_id: Bytes32,
    /// See [`RewardDistributorConstants::fee_payout_puzzle_hash`](crate::RewardDistributorConstants::fee_payout_puzzle_hash).
    pub fee_payout_puzzle_hash: Bytes32,
    /// See [`RewardDistributorConstants::fee_bps`](crate::RewardDistributorConstants::fee_bps).
    pub fee_bps: u64,
    /// See [`RewardDistributorConstants::epoch_seconds`](crate::RewardDistributorConstants::epoch_seconds).
    pub epoch_seconds: u64,
    /// See [`RewardDistributorConstants::precision`](crate::RewardDistributorConstants::precision).
    pub precision: u64,
}

impl ToTreeHash for RewardDistributorNewEpochAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(
            self.launcher_id,
            self.fee_payout_puzzle_hash,
            self.fee_bps,
            self.epoch_seconds,
            self.precision,
        )
        .curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorNewEpochAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            fee_payout_puzzle_hash: constants.fee_payout_puzzle_hash,
            fee_bps: constants.fee_bps,
            epoch_seconds: constants.epoch_seconds,
            precision: constants.precision,
        }
    }
}

impl RewardDistributorNewEpochAction {
    /// Creates the curried arguments of the action puzzle.
    pub fn new_args(
        launcher_id: Bytes32,
        fee_payout_puzzle_hash: Bytes32,
        fee_bps: u64,
        epoch_seconds: u64,
        precision: u64,
    ) -> RewardDistributorNewEpochActionArgs {
        RewardDistributorNewEpochActionArgs {
            reward_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::REWARD.to_u64(),
            )
            .into(),
            fee_payout_puzzle_hash,
            fee_bps,
            epoch_seconds,
            precision,
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(
            self.launcher_id,
            self.fee_payout_puzzle_hash,
            self.fee_bps,
            self.epoch_seconds,
            self.precision,
        ))
    }

    /// Parses an action solution into a [`RewardDistributorNewEpochActionLog`].
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
    ) -> Result<RewardDistributorNewEpochActionLog, DriverError> {
        let params = ctx.extract::<RewardDistributorNewEpochActionSolution>(solution)?;

        Ok(RewardDistributorNewEpochActionLog {
            spent_reward_slot: RewardDistributorRewardSlotValue {
                counter: params.slot_counter,
                epoch_start: params.slot_epoch_time,
                next_epoch_initialized: params.slot_next_epoch_initialized,
                rewards: params.slot_total_rewards,
            },
            created_reward_slot: RewardDistributorRewardSlotValue {
                counter: params.slot_counter + 1,
                epoch_start: params.slot_epoch_time,
                next_epoch_initialized: params.slot_next_epoch_initialized,
                rewards: params.slot_total_rewards,
            },
            epoch_total_rewards: params.epoch_total_rewards,
            changes,
        })
    }

    /// Starts the next epoch in the pending spend of `distributor`, spending `reward_slot`.
    ///
    /// `reward_slot` should be the slot of the epoch starting at the current epoch end. If it's
    /// for another epoch, the new epoch has no rewards.
    ///
    /// Returns the conditions that assert the distributor's new epoch announcement and the
    /// reward slot spend, and the fee paid for the epoch.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the spends can't be created.
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        reward_slot: Slot<RewardDistributorRewardSlotValue>,
    ) -> Result<(Conditions, u64), DriverError> {
        // also returns fee
        let my_state = distributor.pending_spend.latest_state.1;
        let reward_slot = distributor.actual_reward_slot_value(reward_slot);

        let epoch_total_rewards =
            if my_state.round_time_info.epoch_end == reward_slot.info.value.epoch_start {
                reward_slot.info.value.rewards
            } else {
                0
            };
        let fee = epoch_total_rewards * distributor.info.constants.fee_bps / 10000;

        // calculate announcement needed to ensure everything's happening as expected
        let new_epoch_announcement = RewardDistributorCreatedAnnouncementPrefix::new_epoch(
            my_state.round_time_info.epoch_end,
        );
        let new_epoch_conditions = Conditions::new()
            .assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                new_epoch_announcement,
            ))
            .assert_concurrent_puzzle(reward_slot.coin.puzzle_hash);

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorNewEpochActionSolution {
            slot_counter: reward_slot.info.value.counter,
            slot_epoch_time: reward_slot.info.value.epoch_start,
            slot_next_epoch_initialized: reward_slot.info.value.next_epoch_initialized,
            slot_total_rewards: reward_slot.info.value.rewards,
            epoch_total_rewards,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        // spend slot
        reward_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok((new_epoch_conditions, fee))
    }
}
