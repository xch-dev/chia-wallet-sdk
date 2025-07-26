use chia_protocol::Bytes32;
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        RewardDistributorCommitIncentivesActionArgs,
        RewardDistributorCommitIncentivesActionSolution, RewardDistributorCommitmentSlotValue,
        RewardDistributorRewardSlotValue, RewardDistributorSlotNonce,
    },
    Conditions, Mod,
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, RewardDistributor, RewardDistributorConstants, SingletonAction, Slot, Spend,
    SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorCommitIncentivesAction {
    pub launcher_id: Bytes32,
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

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(self.launcher_id, self.epoch_seconds))
    }

    pub fn created_slot_values(
        ctx: &SpendContext,
        epoch_seconds: u64,
        solution: NodePtr,
    ) -> Result<
        (
            RewardDistributorCommitmentSlotValue,
            Vec<RewardDistributorRewardSlotValue>,
        ),
        DriverError,
    > {
        let solution = ctx.extract::<RewardDistributorCommitIncentivesActionSolution>(solution)?;

        let commitment_slot_value = RewardDistributorCommitmentSlotValue {
            epoch_start: solution.epoch_start,
            clawback_ph: solution.clawback_ph,
            rewards: solution.rewards_to_add,
        };

        let mut reward_slot_values = vec![];

        if solution.slot_epoch_time == solution.epoch_start {
            reward_slot_values.push(RewardDistributorRewardSlotValue {
                epoch_start: solution.epoch_start,
                next_epoch_initialized: solution.slot_next_epoch_initialized,
                rewards: solution.slot_total_rewards + solution.rewards_to_add,
            });
        } else {
            reward_slot_values.push(RewardDistributorRewardSlotValue {
                epoch_start: solution.slot_epoch_time,
                next_epoch_initialized: true,
                rewards: solution.slot_total_rewards,
            });
            reward_slot_values.push(RewardDistributorRewardSlotValue {
                epoch_start: solution.epoch_start,
                next_epoch_initialized: false,
                rewards: solution.rewards_to_add,
            });

            let mut start_epoch_time = solution.slot_epoch_time + epoch_seconds;
            let end_epoch_time = solution.epoch_start;
            while end_epoch_time > start_epoch_time {
                reward_slot_values.push(RewardDistributorRewardSlotValue {
                    epoch_start: start_epoch_time,
                    next_epoch_initialized: true,
                    rewards: 0,
                });

                start_epoch_time += epoch_seconds;
            }
        }

        Ok((commitment_slot_value, reward_slot_values))
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorRewardSlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorCommitIncentivesActionSolution>(solution)?;

        Ok(RewardDistributorRewardSlotValue {
            epoch_start: solution.slot_epoch_time,
            next_epoch_initialized: solution.slot_next_epoch_initialized,
            rewards: solution.slot_total_rewards,
        })
    }

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

        let new_commitment_slot_value = RewardDistributorCommitmentSlotValue {
            epoch_start,
            clawback_ph,
            rewards: rewards_to_add,
        };

        // calculate announcement
        let mut commit_reward_announcement = new_commitment_slot_value.tree_hash().to_vec();
        commit_reward_announcement.insert(0, b'c');

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorCommitIncentivesActionSolution {
            slot_epoch_time: reward_slot.info.value.epoch_start,
            slot_next_epoch_initialized: reward_slot.info.value.next_epoch_initialized,
            slot_total_rewards: reward_slot.info.value.rewards,
            epoch_start,
            clawback_ph,
            rewards_to_add,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        // spend reward slot
        reward_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok(
            Conditions::new().assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                commit_reward_announcement,
            )),
        )
    }
}
