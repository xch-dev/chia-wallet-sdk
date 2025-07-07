use chia_protocol::Bytes32;
use chia_sdk_types::{
    announcement_id,
    puzzles::{XchandlesOracleActionArgs, XchandlesSlotValue},
    Conditions, Mod,
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, SingletonAction, Slot, Spend, SpendContext, XchandlesConstants, XchandlesRegistry,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesOracleAction {
    pub launcher_id: Bytes32,
}

impl ToTreeHash for XchandlesOracleAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(self.launcher_id).curry_tree_hash()
    }
}

impl SingletonAction<XchandlesRegistry> for XchandlesOracleAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
        }
    }
}

impl XchandlesOracleAction {
    pub fn new_args(launcher_id: Bytes32) -> XchandlesOracleActionArgs {
        XchandlesOracleActionArgs {
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
        let slot_value = ctx.extract::<XchandlesSlotValue>(solution)?;

        Ok(slot_value)
    }

    pub fn created_slot_value(spent_slot_value: XchandlesSlotValue) -> XchandlesSlotValue {
        spent_slot_value
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        slot: Slot<XchandlesSlotValue>,
    ) -> Result<Conditions, DriverError> {
        // spend self
        let slot = registry.actual_slot(slot);
        let action_solution = ctx.alloc(&slot.info.value)?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        let new_slot = Self::created_slot_value(slot.info.value.clone());

        // spend slot
        slot.spend(ctx, registry.info.inner_puzzle_hash().into())?;

        let mut oracle_ann = new_slot.tree_hash().to_vec();
        oracle_ann.insert(0, b'o');
        Ok(Conditions::new()
            .assert_puzzle_announcement(announcement_id(registry.coin.puzzle_hash, oracle_ann)))
    }
}
