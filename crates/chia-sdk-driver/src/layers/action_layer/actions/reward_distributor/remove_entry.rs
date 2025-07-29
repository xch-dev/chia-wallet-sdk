use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_sdk_types::{
    puzzles::{
        RewardDistributorEntrySlotValue, RewardDistributorRemoveEntryActionArgs,
        RewardDistributorRemoveEntryActionSolution, RewardDistributorSlotNonce,
    },
    Conditions, Mod,
};
use clvm_traits::clvm_tuple;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, RewardDistributor, RewardDistributorConstants, SingletonAction, Slot, Spend,
    SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorRemoveEntryAction {
    pub launcher_id: Bytes32,
    pub manager_launcher_id: Bytes32,
    pub max_seconds_offset: u64,
}

impl ToTreeHash for RewardDistributorRemoveEntryAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(
            self.launcher_id,
            self.manager_launcher_id,
            self.max_seconds_offset,
        )
        .curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorRemoveEntryAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            manager_launcher_id: constants.manager_or_collection_did_launcher_id,
            max_seconds_offset: constants.max_seconds_offset,
        }
    }
}

impl RewardDistributorRemoveEntryAction {
    pub fn new_args(
        launcher_id: Bytes32,
        manager_launcher_id: Bytes32,
        max_seconds_offset: u64,
    ) -> RewardDistributorRemoveEntryActionArgs {
        RewardDistributorRemoveEntryActionArgs {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            manager_singleton_struct_hash: SingletonStruct::new(manager_launcher_id)
                .tree_hash()
                .into(),
            entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::ENTRY.to_u64(),
            )
            .into(),
            max_seconds_offset,
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(
            self.launcher_id,
            self.manager_launcher_id,
            self.max_seconds_offset,
        ))
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        entry_slot: Slot<RewardDistributorEntrySlotValue>,
        manager_singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, u64), DriverError> {
        // u64 = last payment amount
        let my_state = distributor.pending_spend.latest_state.1;
        let entry_slot = distributor.actual_entry_slot_value(entry_slot);

        // compute message that the manager needs to send

        let mut remove_entry_message = clvm_tuple!(
            entry_slot.info.value.payout_puzzle_hash,
            entry_slot.info.value.shares
        )
        .tree_hash()
        .to_vec();
        remove_entry_message.insert(0, b'r');

        let remove_entry_conditions = Conditions::new()
            .send_message(
                18,
                remove_entry_message.into(),
                vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
            )
            .assert_concurrent_puzzle(entry_slot.coin.puzzle_hash);

        // spend self
        let entry_payout_amount = entry_slot.info.value.shares
            * (my_state.round_reward_info.cumulative_payout
                - entry_slot.info.value.initial_cumulative_payout);
        let action_solution = ctx.alloc(&RewardDistributorRemoveEntryActionSolution {
            manager_singleton_inner_puzzle_hash,
            entry_payout_amount,
            entry_payout_puzzle_hash: entry_slot.info.value.payout_puzzle_hash,
            entry_initial_cumulative_payout: entry_slot.info.value.initial_cumulative_payout,
            entry_shares: entry_slot.info.value.shares,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        // spend entry slot
        entry_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok((remove_entry_conditions, entry_payout_amount))
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorRemoveEntryActionSolution>(solution)?;

        Ok(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_payout_puzzle_hash,
            initial_cumulative_payout: solution.entry_initial_cumulative_payout,
            shares: solution.entry_shares,
        })
    }
}
