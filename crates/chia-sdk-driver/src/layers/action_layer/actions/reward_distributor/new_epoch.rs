use chia_protocol::Bytes32;
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        RewardDistributorNewEpochActionArgs, RewardDistributorNewEpochActionSolution,
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
pub struct RewardDistributorNewEpochAction {
    pub launcher_id: Bytes32,
    pub fee_payout_puzzle_hash: Bytes32,
    pub fee_bps: u64,
    pub epoch_seconds: u64,
}

impl ToTreeHash for RewardDistributorNewEpochAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(
            self.launcher_id,
            self.fee_payout_puzzle_hash,
            self.fee_bps,
            self.epoch_seconds,
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
        }
    }
}

impl RewardDistributorNewEpochAction {
    pub fn new_args(
        launcher_id: Bytes32,
        fee_payout_puzzle_hash: Bytes32,
        fee_bps: u64,
        epoch_seconds: u64,
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
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(
            self.launcher_id,
            self.fee_payout_puzzle_hash,
            self.fee_bps,
            self.epoch_seconds,
        ))
    }

    pub fn created_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorRewardSlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorNewEpochActionSolution>(solution)?;

        Ok(RewardDistributorRewardSlotValue {
            epoch_start: solution.slot_epoch_time,
            next_epoch_initialized: solution.slot_next_epoch_initialized,
            rewards: solution.slot_total_rewards,
        })
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorRewardSlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorNewEpochActionSolution>(solution)?;

        Ok(RewardDistributorRewardSlotValue {
            epoch_start: solution.slot_epoch_time,
            next_epoch_initialized: solution.slot_next_epoch_initialized,
            rewards: solution.slot_total_rewards,
        })
    }

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
        let mut new_epoch_announcement = my_state.round_time_info.epoch_end.tree_hash().to_vec();
        new_epoch_announcement.insert(0, b'e');
        let new_epoch_conditions = Conditions::new()
            .assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                new_epoch_announcement,
            ))
            .assert_concurrent_puzzle(reward_slot.coin.puzzle_hash);

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorNewEpochActionSolution {
            slot_epoch_time: reward_slot.info.value.epoch_start,
            slot_next_epoch_initialized: reward_slot.info.value.next_epoch_initialized,
            slot_total_rewards: reward_slot.info.value.rewards,
            epoch_total_rewards,
            fee,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        // spend slot
        reward_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok((new_epoch_conditions, fee))
    }
}
