use chia_protocol::Bytes32;
use chia_sdk_types::{
    Conditions, Mod, announcement_id,
    puzzles::{XchandlesHandleSlotValue, XchandlesOracleActionArgs, XchandlesSlotNonce},
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, SingletonAction, Slot, Spend, SpendContext, XchandlesConstants, XchandlesRegistry,
    XchandlesRegistryCreatedAnnouncementPrefix,
};

use super::XchandlesOracleActionLog;

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
            handle_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                XchandlesSlotNonce::HANDLE.to_u64(),
            )
            .into(),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(self.launcher_id))
    }

    pub fn get_log(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesOracleActionLog, DriverError> {
        let spent_slot = ctx.extract::<XchandlesHandleSlotValue>(solution)?;
        let created_slot = spent_slot.with_counter(spent_slot.counter + 1);

        Ok(XchandlesOracleActionLog {
            spent_slot,
            created_slot,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        slot: Slot<XchandlesHandleSlotValue>,
    ) -> Result<Conditions, DriverError> {
        // spend self
        let slot = registry.actual_handle_slot(slot);
        let action_solution = ctx.alloc(&slot.info.value)?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // let _new_slot = Self::created_slot_value(slot.info.value);

        // spend slot
        let slot_value_hash = slot.info.value.tree_hash();
        slot.spend(ctx, registry.info.inner_puzzle_hash().into())?;

        let oracle_ann = XchandlesRegistryCreatedAnnouncementPrefix::oracle(slot_value_hash);
        Ok(Conditions::new()
            .assert_puzzle_announcement(announcement_id(registry.coin.puzzle_hash, oracle_ann)))
    }
}
