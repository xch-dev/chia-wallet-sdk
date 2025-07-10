use chia_protocol::Bytes32;
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        RewardDistributorEntrySlotValue, RewardDistributorInitiatePayoutActionArgs,
        RewardDistributorInitiatePayoutActionSolution, RewardDistributorSlotNonce,
    },
    Conditions, Mod,
};
use clvm_traits::clvm_tuple;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, RewardDistributor, RewardDistributorConstants, RewardDistributorState,
    SingletonAction, Slot, Spend, SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorInitiatePayoutAction {
    pub launcher_id: Bytes32,
    pub payout_threshold: u64,
}

impl ToTreeHash for RewardDistributorInitiatePayoutAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(self.launcher_id, self.payout_threshold).curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorInitiatePayoutAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            payout_threshold: constants.payout_threshold,
        }
    }
}

impl RewardDistributorInitiatePayoutAction {
    pub fn new_args(
        launcher_id: Bytes32,
        payout_threshold: u64,
    ) -> RewardDistributorInitiatePayoutActionArgs {
        RewardDistributorInitiatePayoutActionArgs {
            entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::ENTRY.to_u64(),
            )
            .into(),
            payout_threshold,
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(self.launcher_id, self.payout_threshold))
    }

    pub fn created_slot_value(
        ctx: &SpendContext,
        current_state: &RewardDistributorState,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorInitiatePayoutActionSolution>(solution)?;

        Ok(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_payout_puzzle_hash,
            initial_cumulative_payout: current_state.round_reward_info.cumulative_payout,
            shares: solution.entry_shares,
        })
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorInitiatePayoutActionSolution>(solution)?;

        Ok(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_payout_puzzle_hash,
            initial_cumulative_payout: solution.entry_initial_cumulative_payout,
            shares: solution.entry_shares,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        entry_slot: Slot<RewardDistributorEntrySlotValue>,
    ) -> Result<(Conditions, u64), DriverError> {
        let my_state = distributor.pending_spend.latest_state.1;
        let entry_slot = distributor.actual_entry_slot_value(entry_slot);

        let withdrawal_amount = entry_slot.info.value.shares
            * (my_state.round_reward_info.cumulative_payout
                - entry_slot.info.value.initial_cumulative_payout);

        // this announcement should be asserted to ensure everything goes according to plan
        let mut initiate_payout_announcement =
            clvm_tuple!(entry_slot.info.value.payout_puzzle_hash, withdrawal_amount)
                .tree_hash()
                .to_vec();
        initiate_payout_announcement.insert(0, b'p');

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorInitiatePayoutActionSolution {
            entry_payout_amount: withdrawal_amount,
            entry_payout_puzzle_hash: entry_slot.info.value.payout_puzzle_hash,
            entry_initial_cumulative_payout: entry_slot.info.value.initial_cumulative_payout,
            entry_shares: entry_slot.info.value.shares,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        // spend entry slot
        entry_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        Ok((
            Conditions::new().assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                initiate_payout_announcement,
            )),
            withdrawal_amount,
        ))
    }
}
