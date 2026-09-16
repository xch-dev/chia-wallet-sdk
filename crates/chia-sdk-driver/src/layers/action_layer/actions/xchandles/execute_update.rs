use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonArgs;
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_sdk_types::{
    Conditions, Mod,
    puzzles::{
        CompactCoinProof, XchandlesDataValue, XchandlesExecuteUpdateActionArgs,
        XchandlesExecuteUpdateActionSolution, XchandlesHandleSlotValue,
        XchandlesNewDataPuzzleHashes, XchandlesSlotNonce, XchandlesUpdateSlotValue,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, SingletonAction, Slot, Spend, SpendContext, XchandlesConstants, XchandlesRegistry,
    XchandlesRegistryReceivedMessagePrefix,
};

use super::{XchandlesExecuteUpdateActionLog, coin_id_from_owner_proof};

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

    pub fn get_log(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesExecuteUpdateActionLog, DriverError> {
        let solution = ctx.extract::<XchandlesExecuteUpdateActionSolution>(solution)?;

        let spent_handle_slot = solution.current_slot_value;
        let spent_update_slot = XchandlesUpdateSlotValue::new(
            solution.current_owner.parent_coin_info,
            solution.min_execution_height,
            solution.current_slot_value.handle_hash,
            solution.new_data.owner_launcher_id,
            solution.new_data.resolved_launcher_id,
        );
        let created_slot = spent_handle_slot
            .with_data(
                solution.new_data.owner_launcher_id,
                solution.new_data.resolved_launcher_id,
            )
            .with_counter(spent_handle_slot.counter + 1);
        let owner_coin_id = coin_id_from_owner_proof(
            solution.current_owner,
            solution.current_slot_value.owner_launcher_id,
        );

        let owner_full_puzzle_hash = SingletonArgs::curry_tree_hash(
            solution.new_data.owner_launcher_id,
            solution
                .new_data_puzzle_hashes
                .new_owner_inner_puzzle_hash
                .into(),
        )
        .into();

        let resolved_full_puzzle_hash =
            if solution.new_data.owner_launcher_id == solution.new_data.resolved_launcher_id {
                None
            } else {
                Some(
                    SingletonArgs::curry_tree_hash(
                        solution.new_data.resolved_launcher_id,
                        solution
                            .new_data_puzzle_hashes
                            .new_resolved_inner_puzzle_hash
                            .into(),
                    )
                    .into(),
                )
            };

        Ok(XchandlesExecuteUpdateActionLog {
            spent_handle_slot,
            spent_update_slot,
            created_slot,
            owner_coin_id,
            owner_full_puzzle_hash,
            resolved_full_puzzle_hash,
            owner_inner_puzzle_hash: solution.new_data_puzzle_hashes.new_owner_inner_puzzle_hash,
            resolved_inner_puzzle_hash: solution
                .new_data_puzzle_hashes
                .new_resolved_inner_puzzle_hash,
        })
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
        current_owner: CompactCoinProof,
        new_owner_inner_puzzle_hash: Bytes32,
        new_resolved_inner_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, Conditions, Conditions), DriverError> {
        // spend self
        let handle_slot = registry.actual_handle_slot(handle_slot);
        let update_slot = registry.actual_update_slot(update_slot);

        let min_execution_height = update_slot.info.value.min_height;

        let action_solution = ctx.alloc(&XchandlesExecuteUpdateActionSolution {
            current_slot_value: handle_slot.info.value,
            new_data: XchandlesDataValue {
                owner_launcher_id: new_owner_launcher_id,
                resolved_launcher_id: new_resolved_launcher_id,
            },
            current_owner,
            min_execution_height,
            new_data_puzzle_hashes: XchandlesNewDataPuzzleHashes::new(
                new_owner_inner_puzzle_hash,
                new_resolved_inner_puzzle_hash,
            ),
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slot
        let my_inner_puzzle_hash = registry.info.inner_puzzle_hash().into();
        let spent_update_slot_value_hash: TreeHash = update_slot.info.value_hash.into();

        handle_slot.spend(ctx, my_inner_puzzle_hash)?;
        update_slot.spend(ctx, my_inner_puzzle_hash)?;

        Ok((
            Conditions::new().send_message(
                58,
                XchandlesRegistryReceivedMessagePrefix::execute_update_old_owner(
                    spent_update_slot_value_hash,
                )
                .into(),
                vec![ctx.alloc(&registry.coin.puzzle_hash)?],
            ),
            Conditions::new().send_message(
                18,
                XchandlesRegistryReceivedMessagePrefix::execute_update_new_owner(
                    spent_update_slot_value_hash,
                )
                .into(),
                vec![ctx.alloc(&registry.coin.puzzle_hash)?],
            ),
            Conditions::new().send_message(
                18,
                XchandlesRegistryReceivedMessagePrefix::execute_update_new_resolved(
                    spent_update_slot_value_hash,
                )
                .into(),
                vec![ctx.alloc(&registry.coin.puzzle_hash)?],
            ),
        ))
    }
}
