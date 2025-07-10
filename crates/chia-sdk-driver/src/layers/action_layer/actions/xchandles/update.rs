use chia_protocol::{Bytes, Bytes32};
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_sdk_types::{
    puzzles::{
        XchandlesDataValue, XchandlesSlotValue, XchandlesUpdateActionArgs,
        XchandlesUpdateActionSolution,
    },
    Conditions, Mod,
};
use clvm_traits::clvm_tuple;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, SingletonAction, Slot, Spend, SpendContext, XchandlesConstants, XchandlesRegistry,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesUpdateAction {
    pub launcher_id: Bytes32,
}

impl ToTreeHash for XchandlesUpdateAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(self.launcher_id).curry_tree_hash()
    }
}

impl SingletonAction<XchandlesRegistry> for XchandlesUpdateAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
        }
    }
}

impl XchandlesUpdateAction {
    pub fn new_args(launcher_id: Bytes32) -> XchandlesUpdateActionArgs {
        XchandlesUpdateActionArgs {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_launcher_mod_hash: SINGLETON_LAUNCHER_HASH.into(),
            slot_1st_curry_hash: Slot::<()>::first_curry_hash(launcher_id, 0).into(),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(self.launcher_id))
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesSlotValue, DriverError> {
        let solution = ctx.extract::<XchandlesUpdateActionSolution>(solution)?;

        Ok(solution.current_slot_value)
    }

    pub fn created_slot_value(
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesSlotValue, DriverError> {
        let solution = ctx.extract::<XchandlesUpdateActionSolution>(solution)?;

        Ok(solution.current_slot_value.with_data(
            solution.new_data.owner_launcher_id,
            solution.new_data.resolved_data,
        ))
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        slot: Slot<XchandlesSlotValue>,
        new_owner_launcher_id: Bytes32,
        new_resolved_data: &Bytes,
        announcer_inner_puzzle_hash: Bytes32,
    ) -> Result<Conditions, DriverError> {
        // spend self
        let slot = registry.actual_slot(slot);
        let action_solution = ctx.alloc(&XchandlesUpdateActionSolution {
            current_slot_value: slot.info.value.clone(),
            new_data: XchandlesDataValue {
                owner_launcher_id: new_owner_launcher_id,
                resolved_data: new_resolved_data.clone(),
            },
            announcer_inner_puzzle_hash,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slot
        let my_inner_puzzle_hash = registry.info.inner_puzzle_hash().into();

        let msg: Bytes32 = clvm_tuple!(
            slot.info.value.handle_hash,
            clvm_tuple!(new_owner_launcher_id, new_resolved_data.clone())
        )
        .tree_hash()
        .into();

        slot.spend(ctx, my_inner_puzzle_hash)?;

        Ok(Conditions::new().send_message(
            18,
            msg.into(),
            vec![ctx.alloc(&registry.coin.puzzle_hash)?],
        ))
    }
}
