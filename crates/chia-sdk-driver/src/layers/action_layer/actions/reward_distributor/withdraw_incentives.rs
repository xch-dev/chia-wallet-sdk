use chia_protocol::Bytes32;
use chia_sdk_types::{
    Conditions, Mod,
    puzzles::{
        RewardDistributorCommitmentSlotValue, RewardDistributorRewardSlotValue,
        RewardDistributorSlotNonce, RewardDistributorWithdrawIncentivesActionArgs,
        RewardDistributorWithdrawIncentivesActionSolution,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, RewardDistributor, RewardDistributorConstants,
    RewardDistributorReceivedMessagePrefix, RewardDistributorStateTransition,
    RewardDistributorWithdrawIncentivesActionLog, SingletonAction, Slot, Spend, SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorWithdrawIncentivesAction {
    pub launcher_id: Bytes32,
    pub withdrawal_share_bps: u64,
}

impl ToTreeHash for RewardDistributorWithdrawIncentivesAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(self.launcher_id, self.withdrawal_share_bps).curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorWithdrawIncentivesAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            withdrawal_share_bps: constants.withdrawal_share_bps,
        }
    }
}

impl RewardDistributorWithdrawIncentivesAction {
    pub fn new_args(
        launcher_id: Bytes32,
        withdrawal_share_bps: u64,
    ) -> RewardDistributorWithdrawIncentivesActionArgs {
        RewardDistributorWithdrawIncentivesActionArgs {
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
            withdrawal_share_bps,
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(self.launcher_id, self.withdrawal_share_bps))
    }

    pub fn get_log(
        ctx: &SpendContext,
        solution: NodePtr,
        changes: RewardDistributorStateTransition,
        withdrawal_share_bps: u64,
    ) -> Result<RewardDistributorWithdrawIncentivesActionLog, DriverError> {
        let params = ctx.extract::<RewardDistributorWithdrawIncentivesActionSolution>(solution)?;
        let withdrawal_share = params.committed_value * withdrawal_share_bps / 10000;

        Ok(RewardDistributorWithdrawIncentivesActionLog {
            spent_reward_slot: RewardDistributorRewardSlotValue {
                counter: params.reward_slot_counter,
                epoch_start: params.reward_slot_epoch_time,
                next_epoch_initialized: params.reward_slot_next_epoch_initialized,
                rewards: params.reward_slot_total_rewards,
            },
            spent_commitment_slot: RewardDistributorCommitmentSlotValue {
                epoch_start: params.reward_slot_epoch_time,
                clawback_ph: params.clawback_ph,
                rewards: params.committed_value,
            },
            created_reward_slot: RewardDistributorRewardSlotValue {
                counter: params.reward_slot_counter + 1,
                epoch_start: params.reward_slot_epoch_time,
                next_epoch_initialized: params.reward_slot_next_epoch_initialized,
                rewards: params.reward_slot_total_rewards - withdrawal_share,
            },
            changes,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        commitment_slot: Slot<RewardDistributorCommitmentSlotValue>,
        reward_slot: Slot<RewardDistributorRewardSlotValue>,
    ) -> Result<(Conditions, u64), DriverError> {
        // last u64 = withdrawn amount
        let commitment_slot = distributor.actual_commitment_slot_value(commitment_slot);
        let reward_slot = distributor.actual_reward_slot_value(reward_slot);
        let withdrawal_share = commitment_slot.info.value.rewards
            * distributor.info.constants.withdrawal_share_bps
            / 10000;

        // calculate message that the withdrawer needs to send
        let withdraw_incentives_conditions = Conditions::new()
            .send_message(
                18,
                RewardDistributorReceivedMessagePrefix::withdraw_incentives(
                    reward_slot.info.value.epoch_start,
                    commitment_slot.info.value.rewards,
                )
                .into(),
                vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
            )
            .assert_concurrent_puzzle(commitment_slot.coin.puzzle_hash);

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorWithdrawIncentivesActionSolution {
            reward_slot_counter: reward_slot.info.value.counter,
            reward_slot_epoch_time: reward_slot.info.value.epoch_start,
            reward_slot_next_epoch_initialized: reward_slot.info.value.next_epoch_initialized,
            reward_slot_total_rewards: reward_slot.info.value.rewards,
            clawback_ph: commitment_slot.info.value.clawback_ph,
            committed_value: commitment_slot.info.value.rewards,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slots
        let my_inner_puzzle_hash = distributor.info.inner_puzzle_hash().into();
        reward_slot.spend(ctx, my_inner_puzzle_hash)?;
        commitment_slot.spend(ctx, my_inner_puzzle_hash)?;

        Ok((withdraw_incentives_conditions, withdrawal_share))
    }
}
