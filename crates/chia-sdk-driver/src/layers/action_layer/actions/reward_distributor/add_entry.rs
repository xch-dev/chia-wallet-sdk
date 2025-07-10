use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_sdk_types::{
    puzzles::{
        RewardDistributorAddEntryActionArgs, RewardDistributorAddEntryActionSolution,
        RewardDistributorEntrySlotValue, RewardDistributorSlotNonce,
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
pub struct RewardDistributorAddEntryAction {
    pub launcher_id: Bytes32,
    pub manager_launcher_id: Bytes32,
    pub max_second_offset: u64,
}

impl ToTreeHash for RewardDistributorAddEntryAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(
            self.launcher_id,
            self.manager_launcher_id,
            self.max_second_offset,
        )
        .curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorAddEntryAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            manager_launcher_id: constants.manager_or_collection_did_launcher_id,
            max_second_offset: constants.max_seconds_offset,
        }
    }
}

impl RewardDistributorAddEntryAction {
    pub fn new_args(
        launcher_id: Bytes32,
        manager_launcher_id: Bytes32,
        max_second_offset: u64,
    ) -> RewardDistributorAddEntryActionArgs {
        RewardDistributorAddEntryActionArgs {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            manager_singleton_struct_hash: SingletonStruct::new(manager_launcher_id)
                .tree_hash()
                .into(),
            entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::ENTRY.to_u64(),
            )
            .into(),
            max_second_offset,
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(
            self.launcher_id,
            self.manager_launcher_id,
            self.max_second_offset,
        ))
    }

    pub fn created_slot_value(
        ctx: &SpendContext,
        state: &RewardDistributorState,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorAddEntryActionSolution>(solution)?;

        Ok(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_payout_puzzle_hash,
            initial_cumulative_payout: state.round_reward_info.cumulative_payout,
            shares: solution.entry_shares,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        payout_puzzle_hash: Bytes32,
        shares: u64,
        manager_singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<Conditions, DriverError> {
        // calculate message that the manager needs to send
        let mut add_entry_message = clvm_tuple!(payout_puzzle_hash, shares).tree_hash().to_vec();
        add_entry_message.insert(0, b'a');
        let add_entry_message = Conditions::new().send_message(
            18,
            add_entry_message.into(),
            vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
        );

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorAddEntryActionSolution {
            manager_singleton_inner_puzzle_hash,
            entry_payout_puzzle_hash: payout_puzzle_hash,
            entry_shares: shares,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok(add_entry_message)
    }
}
