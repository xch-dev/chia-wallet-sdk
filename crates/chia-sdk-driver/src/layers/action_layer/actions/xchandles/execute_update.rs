use chia_protocol::Bytes32;
use chia_puzzle_types::CoinProof;
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_sdk_types::{
    puzzles::{
        XchandlesDataValue, XchandlesExecuteUpdateActionArgs, XchandlesExecuteUpdateActionSolution,
        XchandlesHandleSlotValue, XchandlesInitiateUpdateActionSolution, XchandlesSlotNonce,
        XchandlesUpdateSlotValue,
    },
    Conditions, Mod,
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, SingletonAction, Slot, Spend, SpendContext, XchandlesConstants, XchandlesRegistry,
    XchandlesRegistryReceivedMessagePrefix,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesExecuteUpdateAction {
    pub launcher_id: Bytes32,
}

impl ToTreeHash for XchandlesExecuteUpdateAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(self.launcher_id).curry_tree_hash()
    }
}

impl SingletonAction<XchandlesRegistry> for XchandlesExecuteUpdateAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
        }
    }
}

impl XchandlesExecuteUpdateAction {
    pub fn new_args(launcher_id: Bytes32) -> XchandlesExecuteUpdateActionArgs {
        XchandlesExecuteUpdateActionArgs {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_launcher_mod_hash: SINGLETON_LAUNCHER_HASH.into(),
            handle_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                XchandlesSlotNonce::HANDLE.to_u64(),
            )
            .into(),
            update_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                XchandlesSlotNonce::UPDATE.to_u64(),
            )
            .into(),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(self.launcher_id))
    }

    pub fn spent_slot_values(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<(XchandlesHandleSlotValue, XchandlesUpdateSlotValue), DriverError> {
        let solution = ctx.extract::<XchandlesInitiateUpdateActionSolution>(solution)?;

        Ok((
            solution.current_slot_value,
            XchandlesUpdateSlotValue::new(
                solution.current_owner.parent_coin_info,
                solution.min_height,
                solution.current_slot_value.handle_hash,
                solution.new_data.owner_launcher_id,
                solution.new_data.resolved_launcher_id,
            ),
        ))
    }

    pub fn created_slot_value(
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesHandleSlotValue, DriverError> {
        let solution = ctx.extract::<XchandlesInitiateUpdateActionSolution>(solution)?;

        Ok(solution.current_slot_value.with_data(
            solution.new_data.owner_launcher_id,
            solution.new_data.resolved_launcher_id,
        ))
    }

    // returns:
    //  - message to be sent by old owner
    //  - message to be sent by new owner
    //  - message to be sent by new resolved
    #[allow(clippy::too_many_arguments)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        handle_slot: Slot<XchandlesHandleSlotValue>,
        update_slot: Slot<XchandlesUpdateSlotValue>,
        new_owner_launcher_id: Bytes32,
        new_resolved_launcher_id: Bytes32,
        current_owner: CoinProof,
        min_execution_height: u64,
        new_owner_inner_puzzle_hash: Bytes32,
        new_resolved_inner_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, Conditions, Conditions), DriverError> {
        // spend self
        let handle_slot = registry.actual_handle_slot(handle_slot);
        let update_slot = registry.actual_update_slot(update_slot);

        let action_solution = ctx.alloc(&XchandlesExecuteUpdateActionSolution {
            current_slot_value: handle_slot.info.value,
            new_data: XchandlesDataValue {
                owner_launcher_id: new_owner_launcher_id,
                resolved_launcher_id: new_resolved_launcher_id,
            },
            current_owner,
            min_execution_height,
            new_owner_inner_puzzle_hash,
            new_resolved_inner_puzzle_hash,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slot
        let my_inner_puzzle_hash = registry.info.inner_puzzle_hash().into();
        let handle_hash = handle_slot.info.value.handle_hash;

        handle_slot.spend(ctx, my_inner_puzzle_hash)?;
        update_slot.spend(ctx, my_inner_puzzle_hash)?;

        Ok((
            Conditions::new().send_message(
                58,
                XchandlesRegistryReceivedMessagePrefix::execute_update_old_owner(
                    handle_hash,
                    new_owner_launcher_id,
                    new_resolved_launcher_id,
                )
                .into(),
                vec![ctx.alloc(&registry.coin.puzzle_hash)?],
            ),
            Conditions::new().send_message(
                18,
                XchandlesRegistryReceivedMessagePrefix::execute_update_new_owner(
                    handle_hash,
                    new_owner_launcher_id,
                    new_resolved_launcher_id,
                )
                .into(),
                vec![ctx.alloc(&registry.coin.puzzle_hash)?],
            ),
            Conditions::new().send_message(
                18,
                XchandlesRegistryReceivedMessagePrefix::execute_update_new_resolved(
                    handle_hash,
                    new_owner_launcher_id,
                    new_resolved_launcher_id,
                )
                .into(),
                vec![ctx.alloc(&registry.coin.puzzle_hash)?],
            ),
        ))
    }
}
