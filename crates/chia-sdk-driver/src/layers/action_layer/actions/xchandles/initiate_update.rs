use chia_protocol::Bytes32;
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_sdk_types::{
    Conditions, Mod,
    puzzles::{
        CompactCoinProof, XchandlesDataValue, XchandlesHandleSlotValue,
        XchandlesInitiateUpdateActionArgs, XchandlesInitiateUpdateActionSolution,
        XchandlesSlotNonce, XchandlesUpdateSlotValue,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, SingletonAction, Slot, Spend, SpendContext, XchandlesConstants, XchandlesRegistry,
    XchandlesRegistryReceivedMessagePrefix,
};

use super::{XchandlesInitiateUpdateActionLog, coin_id_from_owner_proof};

/// The `XCHandles` action that starts changing the owner and resolved launcher ids of a handle.
///
/// Updates take two steps. This action is authorized by the current owner singleton and creates
/// an update slot with the new data. The update slot stores a minimum height, and the update can
/// only be applied with an [`XchandlesExecuteUpdateAction`](crate::XchandlesExecuteUpdateAction)
/// once that height is reached.
///
/// Use [`SingletonAction::from_constants`] to create it from a registry's constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesInitiateUpdateAction {
    /// The launcher id of the `XCHandles` registry.
    pub launcher_id: Bytes32,
    /// The number of blocks added to the `min_height` in the solution to get the update slot's
    /// minimum height.
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
    /// Creates the curried arguments of the action puzzle.
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

    /// Computes the value of the update slot created by an action with the given solution.
    pub fn created_update_slot_value_from_solution(
        solution: &XchandlesInitiateUpdateActionSolution,
        relative_block_height: u32,
    ) -> XchandlesUpdateSlotValue {
        let initiator_coin_id = coin_id_from_owner_proof(
            solution.current_owner,
            solution.current_slot_value.owner_launcher_id,
        );

        XchandlesUpdateSlotValue::new(
            initiator_coin_id,
            solution.min_height + relative_block_height,
            solution.current_slot_value.handle_hash,
            solution.new_data.owner_launcher_id,
            solution.new_data.resolved_launcher_id,
        )
    }

    /// Parses an action solution into an [`XchandlesInitiateUpdateActionLog`].
    ///
    /// `constants` are the registry's constants, which provide the relative block height.
    ///
    /// # Errors
    ///
    /// Returns an error if the solution can't be parsed.
    pub fn get_log(
        ctx: &SpendContext,
        solution: NodePtr,
        constants: &XchandlesConstants,
    ) -> Result<XchandlesInitiateUpdateActionLog, DriverError> {
        let solution = ctx.extract::<XchandlesInitiateUpdateActionSolution>(solution)?;

        let spent_slot = solution.current_slot_value;
        let created_handle_slot = spent_slot.with_counter(spent_slot.counter + 1);
        let initiator_coin_id = coin_id_from_owner_proof(
            solution.current_owner,
            solution.current_slot_value.owner_launcher_id,
        );
        let created_update_slot = Self::created_update_slot_value_from_solution(
            &solution,
            constants.relative_block_height,
        );

        Ok(XchandlesInitiateUpdateActionLog {
            spent_slot,
            created_handle_slot,
            created_update_slot,
            initiator_coin_id,
        })
    }

    /// Starts updating the handle in `slot` in the pending spend of `registry`.
    ///
    /// `current_owner` is a proof of the current owner singleton coin, which must send the
    /// returned message. The update slot's minimum height is `min_height` plus
    /// [`relative_block_height`](Self::relative_block_height).
    ///
    /// Returns the `SEND_MESSAGE` condition (mode 58) that the current owner singleton must
    /// output to authorize the update.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the spends can't be created.
    #[allow(clippy::too_many_arguments)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        slot: Slot<XchandlesHandleSlotValue>,
        new_owner_launcher_id: Bytes32,
        new_resolved_launcher_id: Bytes32,
        current_owner: CompactCoinProof,
        min_height: u32,
    ) -> Result<Conditions, DriverError> {
        // spend self
        let slot = registry.actual_handle_slot(slot);
        let action_solution = XchandlesInitiateUpdateActionSolution {
            current_slot_value: slot.info.value,
            new_data: XchandlesDataValue {
                owner_launcher_id: new_owner_launcher_id,
                resolved_launcher_id: new_resolved_launcher_id,
            },
            current_owner,
            min_height,
        };
        let created_update_slot = Self::created_update_slot_value_from_solution(
            &action_solution,
            self.relative_block_height,
        );

        let action_solution = ctx.alloc(&action_solution)?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slot
        let my_inner_puzzle_hash = registry.info.inner_puzzle_hash().into();

        slot.spend(ctx, my_inner_puzzle_hash)?;

        Ok(Conditions::new().send_message(
            58,
            XchandlesRegistryReceivedMessagePrefix::initiate_update(
                created_update_slot.tree_hash(),
            )
            .into(),
            vec![ctx.alloc(&registry.coin.puzzle_hash)?],
        ))
    }
}
