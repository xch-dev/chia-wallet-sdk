use chia_protocol::Bytes32;
use chia_puzzle_types::offer::{NotarizedPayment, Payment};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    Conditions, Mod, announcement_id,
    puzzles::{
        DefaultCatMakerArgs, PuzzleAndSolution, XchandlesExtendActionArgs,
        XchandlesExtendActionSolution, XchandlesFactorPricingPuzzleArgs, XchandlesHandleSlotValue,
        XchandlesPricingSolution, XchandlesSlotNonce,
    },
};
use clvm_traits::clvm_tuple;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, SingletonAction, Slot, Spend, SpendContext, XchandlesConstants, XchandlesRegistry,
    XchandlesRegistryCreatedAnnouncementPrefix,
};

use super::{XchandlesExtendActionLog, run_pricing_output};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesExtendAction {
    pub launcher_id: Bytes32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for XchandlesExtendAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(self.launcher_id, self.payout_puzzle_hash).curry_tree_hash()
    }
}

impl SingletonAction<XchandlesRegistry> for XchandlesExtendAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            payout_puzzle_hash: constants.precommit_payout_puzzle_hash,
        }
    }
}

impl XchandlesExtendAction {
    pub fn new_args(
        launcher_id: Bytes32,
        payout_puzzle_hash: Bytes32,
    ) -> XchandlesExtendActionArgs {
        XchandlesExtendActionArgs {
            offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
            payout_puzzle_hash,
            handle_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                XchandlesSlotNonce::HANDLE.to_u64(),
            )
            .into(),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(self.launcher_id, self.payout_puzzle_hash))
    }

    pub fn get_log(
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesExtendActionLog, DriverError> {
        let solution =
            ctx.extract::<XchandlesExtendActionSolution<NodePtr, NodePtr, NodePtr, ()>>(solution)?;

        let pricing_solution =
            ctx.extract::<XchandlesPricingSolution>(solution.pricing_puzzle_and_solution.solution)?;
        let spent_slot = XchandlesHandleSlotValue::new(
            solution.counter,
            pricing_solution.handle.tree_hash().into(),
            solution.neighbors.left_value,
            solution.neighbors.right_value,
            pricing_solution.current_expiration,
            solution.rest.owner_launcher_id,
            solution.rest.resolved_launcher_id,
        );

        let (total_price, registered_time) = run_pricing_output(
            ctx,
            solution.pricing_puzzle_and_solution.puzzle,
            solution.pricing_puzzle_and_solution.solution,
        )?;

        let created_slot = XchandlesHandleSlotValue::new(
            solution.counter + 1,
            spent_slot.handle_hash,
            solution.neighbors.left_value,
            solution.neighbors.right_value,
            pricing_solution.current_expiration + registered_time,
            solution.rest.owner_launcher_id,
            solution.rest.resolved_launcher_id,
        );

        Ok(XchandlesExtendActionLog {
            spent_slot,
            created_slot,
            total_price,
            registered_time,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        handle: &str,
        slot: Slot<XchandlesHandleSlotValue>,
        payment_asset_id: Bytes32,
        base_handle_price: u64,
        registration_period: u64,
        num_periods: u64,
        buy_time: u64,
    ) -> Result<(Conditions, NotarizedPayment), DriverError> {
        let spender_inner_puzzle_hash = registry.info.inner_puzzle_hash().into();

        // spend self
        let cat_maker_puzzle_reveal = ctx.curry(DefaultCatMakerArgs::new(
            payment_asset_id.tree_hash().into(),
        ))?;
        let pricing_puzzle_reveal = ctx.curry(XchandlesFactorPricingPuzzleArgs {
            base_price: base_handle_price,
            registration_period,
        })?;

        let slot = registry.actual_handle_slot(slot);
        let action_solution = ctx.alloc(&XchandlesExtendActionSolution {
            counter: slot.info.value.counter,
            pricing_puzzle_and_solution: PuzzleAndSolution::new(
                pricing_puzzle_reveal,
                XchandlesPricingSolution {
                    buy_time,
                    current_expiration: slot.info.value.expiration,
                    handle: handle.to_string(),
                    num_periods,
                },
            ),
            neighbors: slot.info.value.neighbors,
            rest: slot.info.value.rest_data(),
            cat_maker_and_solution: PuzzleAndSolution::new(cat_maker_puzzle_reveal, ()),
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        let renew_amount =
            XchandlesFactorPricingPuzzleArgs::get_price(base_handle_price, handle, num_periods);

        let notarized_payment = NotarizedPayment {
            nonce: clvm_tuple!(handle.to_string(), slot.info.value.expiration)
                .tree_hash()
                .into(),
            payments: vec![Payment::new(
                registry.info.constants.precommit_payout_puzzle_hash,
                renew_amount,
                ctx.hint(registry.info.constants.precommit_payout_puzzle_hash)?,
            )],
        };

        // spend slot
        slot.spend(ctx, spender_inner_puzzle_hash)?;

        Ok((
            Conditions::new().assert_puzzle_announcement(announcement_id(
                registry.coin.puzzle_hash,
                XchandlesRegistryCreatedAnnouncementPrefix::extend(renew_amount, handle),
            )),
            notarized_payment,
        ))
    }
}
