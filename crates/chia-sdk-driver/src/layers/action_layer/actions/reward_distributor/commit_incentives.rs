use chia_consensus::opcodes::NEW_CREATE_COIN_COST;
use chia_protocol::Bytes32;
use chia_sdk_types::{
    Conditions, MAINNET_CONSTANTS, Mod, announcement_id,
    puzzles::{
        RewardDistributorCommitIncentivesActionArgs,
        RewardDistributorCommitIncentivesActionSolution, RewardDistributorCommitmentSlotValue,
        RewardDistributorRewardSlotValue, RewardDistributorSlotNonce,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use super::checked_add;

use crate::{
    DriverError, RewardDistributor, RewardDistributorCommitIncentivesActionLog,
    RewardDistributorConstants, RewardDistributorCreatedAnnouncementPrefix,
    RewardDistributorStateTransition, SingletonAction, Slot, Spend, SpendContext,
};

/// The reward distributor action that commits incentives to a future epoch.
///
/// It adds the rewards to the epoch's reward slot, creating the reward slots of any epochs in
/// between, and creates a commitment slot that records the rewards and a clawback puzzle hash
/// so that a share can be withdrawn later with a
/// [`RewardDistributorWithdrawIncentivesAction`](crate::RewardDistributorWithdrawIncentivesAction).
///
/// Use [`SingletonAction::from_constants`] to create it from a distributor's constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorCommitIncentivesAction {
    /// The launcher id of the reward distributor.
    pub launcher_id: Bytes32,
    /// See [`RewardDistributorConstants::epoch_seconds`](crate::RewardDistributorConstants::epoch_seconds).
    pub epoch_seconds: u64,
}

impl ToTreeHash for RewardDistributorCommitIncentivesAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(self.launcher_id, self.epoch_seconds).curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorCommitIncentivesAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            epoch_seconds: constants.epoch_seconds,
        }
    }
}

impl RewardDistributorCommitIncentivesAction {
    /// Creates the curried arguments of the action puzzle.
    pub fn new_args(
        launcher_id: Bytes32,
        epoch_seconds: u64,
    ) -> RewardDistributorCommitIncentivesActionArgs {
        RewardDistributorCommitIncentivesActionArgs {
            reward_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::REWARD.to_u64(),
            )
            .into(),
            commitment_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::COMMITMENT.to_u64(),
            )
            .into(),
            epoch_seconds,
        }
    }

    /// Returns the number of epochs from the spent reward slot's epoch, `slot_epoch_time`, to the
    /// committed epoch, `epoch_start`.
    ///
    /// If they differ, the puzzle steps through the epochs until it reaches `epoch_start`
    /// exactly, so it must be a positive whole number of epochs later. The puzzle creates a slot
    /// for each of these epochs, plus the updated spent reward slot and the commitment slot. Each
    /// slot is a `CREATE_COIN`, so a spend that creates more slots than a mainnet block's cost
    /// limit divided by the cost of a `CREATE_COIN` (8,148 since the 3.0 hard fork lowered that
    /// cost) can't be confirmed, and is rejected rather than parsed.
    fn epochs_to_target(
        slot_epoch_time: u64,
        epoch_start: u64,
        epoch_seconds: u64,
    ) -> Result<u64, DriverError> {
        let max_created_slots = MAINNET_CONSTANTS.max_block_cost_clvm / NEW_CREATE_COIN_COST;

        let elapsed = epoch_start
            .checked_sub(slot_epoch_time)
            .ok_or(DriverError::InvalidRewardDistributorEpoch)?;

        if elapsed == 0 {
            return Ok(0);
        }

        if epoch_seconds == 0 || elapsed % epoch_seconds != 0 {
            return Err(DriverError::InvalidRewardDistributorEpoch);
        }

        let epochs = elapsed / epoch_seconds;

        if epochs > max_created_slots - 2 {
            return Err(DriverError::InvalidRewardDistributorEpoch);
        }

        Ok(epochs)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(self.launcher_id, self.epoch_seconds))
    }

    /// Parses an action solution into a [`RewardDistributorCommitIncentivesActionLog`].
    ///
    /// `changes` is the distributor state before and after the action, and `epoch_seconds` is
    /// the distributor's epoch length.
    ///
    /// # Errors
    ///
    /// Returns an error if the solution can't be parsed, with
    /// [`DriverError::ArithmeticOverflow`] if the slot counter or the epoch's rewards overflow, or
    /// with [`DriverError::InvalidRewardDistributorEpoch`] if the spend targets an epoch that the
    /// puzzle can't reach from the spent reward slot (see [`DriverError::InvalidRewardDistributorEpoch`]).
    pub fn get_log(
        ctx: &SpendContext,
        solution: NodePtr,
        changes: RewardDistributorStateTransition,
        epoch_seconds: u64,
    ) -> Result<RewardDistributorCommitIncentivesActionLog, DriverError> {
        let params = ctx.extract::<RewardDistributorCommitIncentivesActionSolution>(solution)?;

        let created_commitment_slot = RewardDistributorCommitmentSlotValue {
            epoch_start: params.epoch_start,
            clawback_ph: params.clawback_ph,
            rewards: params.rewards_to_add,
        };

        let epochs =
            Self::epochs_to_target(params.slot_epoch_time, params.epoch_start, epoch_seconds)?;
        let counter = checked_add(params.slot_counter, 1)?;

        let created_reward_slots = if epochs == 0 {
            vec![RewardDistributorRewardSlotValue {
                counter,
                epoch_start: params.epoch_start,
                next_epoch_initialized: params.slot_next_epoch_initialized,
                rewards: checked_add(params.slot_total_rewards, params.rewards_to_add)?,
            }]
        } else {
            let mut created_reward_slots = vec![
                RewardDistributorRewardSlotValue {
                    counter,
                    epoch_start: params.slot_epoch_time,
                    next_epoch_initialized: true,
                    rewards: params.slot_total_rewards,
                },
                RewardDistributorRewardSlotValue {
                    counter: 0,
                    epoch_start: params.epoch_start,
                    next_epoch_initialized: false,
                    rewards: params.rewards_to_add,
                },
            ];

            // Every epoch here starts before `epoch_start`, so this can't overflow.
            created_reward_slots.extend((1..epochs).map(|epoch| {
                RewardDistributorRewardSlotValue {
                    counter: 0,
                    epoch_start: params.slot_epoch_time + epoch * epoch_seconds,
                    next_epoch_initialized: true,
                    rewards: 0,
                }
            }));

            created_reward_slots
        };

        Ok(RewardDistributorCommitIncentivesActionLog {
            spent_reward_slot: RewardDistributorRewardSlotValue {
                counter: params.slot_counter,
                epoch_start: params.slot_epoch_time,
                next_epoch_initialized: params.slot_next_epoch_initialized,
                rewards: params.slot_total_rewards,
            },
            created_commitment_slot,
            created_reward_slots,
            changes,
        })
    }

    /// Commits `rewards_to_add` to the epoch starting at `epoch_start` in the pending spend of
    /// `distributor`, spending `reward_slot`.
    ///
    /// `reward_slot` is the slot of the committed epoch if it exists, or else the latest reward
    /// slot. `clawback_ph` is the puzzle hash that can withdraw the commitment.
    ///
    /// Returns a condition that asserts the distributor's commit incentives announcement.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the spends can't be created, or for the reasons listed in
    /// [`get_log`](Self::get_log), such as an `epoch_start` that isn't a whole number of epochs
    /// after the epoch of `reward_slot`.
    #[allow(clippy::type_complexity)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        reward_slot: Slot<RewardDistributorRewardSlotValue>,
        epoch_start: u64,
        clawback_ph: Bytes32,
        rewards_to_add: u64,
    ) -> Result<Conditions, DriverError> {
        let reward_slot = distributor.actual_reward_slot_value(reward_slot);

        // Fail before running the puzzle, which recurses until it runs out of memory for an
        // unreachable epoch.
        Self::epochs_to_target(
            reward_slot.info.value.epoch_start,
            epoch_start,
            self.epoch_seconds,
        )?;

        let new_commitment_slot_value = RewardDistributorCommitmentSlotValue {
            epoch_start,
            clawback_ph,
            rewards: rewards_to_add,
        };

        // calculate announcement
        let commit_reward_announcement =
            RewardDistributorCreatedAnnouncementPrefix::commit_incentives(
                new_commitment_slot_value.tree_hash(),
            );

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorCommitIncentivesActionSolution {
            slot_counter: reward_slot.info.value.counter,
            slot_epoch_time: reward_slot.info.value.epoch_start,
            slot_next_epoch_initialized: reward_slot.info.value.next_epoch_initialized,
            slot_total_rewards: reward_slot.info.value.rewards,
            epoch_start,
            clawback_ph,
            rewards_to_add,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend reward slot
        reward_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;
        Ok(
            Conditions::new().assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                commit_reward_announcement,
            )),
        )
    }
}
