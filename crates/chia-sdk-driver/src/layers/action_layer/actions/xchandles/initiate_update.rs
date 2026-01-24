use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{singleton::SingletonArgs, CoinProof};
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_sdk_types::{
    puzzles::{
        XchandlesDataValue, XchandlesHandleSlotValue, XchandlesInitiateUpdateActionArgs,
        XchandlesInitiateUpdateActionSolution, XchandlesSlotNonce, XchandlesUpdateSlotValue,
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
pub struct XchandlesInitiateUpdateAction {
    pub launcher_id: Bytes32,
    pub relative_block_height: u32,
}

impl ToTreeHash for XchandlesInitiateUpdateAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(self.launcher_id, self.relative_block_height).curry_tree_hash()
    }
}

impl SingletonAction<XchandlesRegistry> for XchandlesInitiateUpdateAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            relative_block_height: constants.relative_block_height,
        }
    }
}

impl XchandlesInitiateUpdateAction {
    pub fn new_args(
        launcher_id: Bytes32,
        relative_block_height: u32,
    ) -> XchandlesInitiateUpdateActionArgs {
        XchandlesInitiateUpdateActionArgs {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_launcher_mod_hash: SINGLETON_LAUNCHER_HASH.into(),
            relative_block_height,
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
        ctx.curry(Self::new_args(self.launcher_id, self.relative_block_height))
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesHandleSlotValue, DriverError> {
        let solution = ctx.extract::<XchandlesInitiateUpdateActionSolution>(solution)?;

        Ok(solution.current_slot_value)
    }

    pub fn created_slot_values(
        ctx: &mut SpendContext,
        solution: NodePtr,
        relative_block_height: u32,
    ) -> Result<(XchandlesHandleSlotValue, XchandlesUpdateSlotValue), DriverError> {
        let solution = ctx.extract::<XchandlesInitiateUpdateActionSolution>(solution)?;

        Ok((
            solution.current_slot_value,
            XchandlesUpdateSlotValue::new(
                Coin::new(
                    solution.current_owner.parent_coin_info,
                    SingletonArgs::curry_tree_hash(
                        solution.current_slot_value.owner_launcher_id,
                        solution.current_owner.inner_puzzle_hash.into(),
                    )
                    .into(),
                    solution.current_owner.amount,
                )
                .coin_id(),
                solution.min_height + u64::from(relative_block_height),
                solution.current_slot_value.handle_hash,
                solution.new_data.owner_launcher_id,
                solution.new_data.resolved_launcher_id,
            ),
        ))
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        slot: Slot<XchandlesHandleSlotValue>,
        new_owner_launcher_id: Bytes32,
        new_resolved_launcher_id: Bytes32,
        current_owner: CoinProof,
        min_height: u64,
    ) -> Result<Conditions, DriverError> {
        // spend self
        let slot = registry.actual_handle_slot(slot);
        let action_solution = ctx.alloc(&XchandlesInitiateUpdateActionSolution {
            current_slot_value: slot.info.value.clone(),
            new_data: XchandlesDataValue {
                owner_launcher_id: new_owner_launcher_id,
                resolved_launcher_id: new_resolved_launcher_id,
            },
            current_owner,
            min_height,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slot
        let my_inner_puzzle_hash = registry.info.inner_puzzle_hash().into();
        let handle_hash = slot.info.value.handle_hash;

        slot.spend(ctx, my_inner_puzzle_hash)?;

        Ok(Conditions::new().send_message(
            58,
            XchandlesRegistryReceivedMessagePrefix::initiate_update(
                handle_hash,
                new_owner_launcher_id,
                new_resolved_launcher_id,
            )
            .into(),
            vec![ctx.alloc(&registry.coin.puzzle_hash)?],
        ))
    }
}
