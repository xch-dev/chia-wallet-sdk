use chia_protocol::Bytes32;
use chia_puzzle_types::offer::{NotarizedPayment, Payment};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        DefaultCatMakerArgs, XchandlesExtendActionArgs, XchandlesExtendActionSolution,
        XchandlesFactorPricingPuzzleArgs, XchandlesPricingSolution, XchandlesSlotValue,
    },
    Conditions, Mod,
};
use clvm_traits::{clvm_tuple, FromClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, SingletonAction, Slot, Spend, SpendContext, XchandlesConstants, XchandlesRegistry,
};

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
            slot_1st_curry_hash: Slot::<()>::first_curry_hash(launcher_id, 0).into(),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(self.launcher_id, self.payout_puzzle_hash))
    }

    pub fn spent_slot_value(
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesSlotValue, DriverError> {
        let solution = ctx.extract::<XchandlesExtendActionSolution<
            NodePtr,
            (u64, (u64, (String, NodePtr))),
            NodePtr,
            NodePtr,
        >>(solution)?;

        // current expiration is the second truth given to a pricing puzzle
        let current_expiration = solution.pricing_solution.1 .0;

        Ok(XchandlesSlotValue::new(
            solution.pricing_solution.1 .1 .0.tree_hash().into(),
            solution.neighbors.left_value,
            solution.neighbors.right_value,
            current_expiration,
            solution.rest.owner_launcher_id,
            solution.rest.resolved_data,
        ))
    }

    pub fn created_slot_value(
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesSlotValue, DriverError> {
        let solution = ctx
            .extract::<XchandlesExtendActionSolution<NodePtr, NodePtr, NodePtr, NodePtr>>(
                solution,
            )?;

        let pricing_output = ctx.run(solution.pricing_puzzle_reveal, solution.pricing_solution)?;
        let registration_time_delta = <(NodePtr, u64)>::from_clvm(ctx, pricing_output)?.1;

        let (_, (_, (handle, _))) =
            ctx.extract::<(NodePtr, (NodePtr, (String, NodePtr)))>(solution.pricing_solution)?;

        // current expiration is the second truth given to a pricing puzzle
        let current_expiration = ctx
            .extract::<(NodePtr, (u64, NodePtr))>(solution.pricing_solution)?
            .1
             .0;

        Ok(XchandlesSlotValue::new(
            handle.tree_hash().into(),
            solution.neighbors.left_value,
            solution.neighbors.right_value,
            current_expiration + registration_time_delta,
            solution.rest.owner_launcher_id,
            solution.rest.resolved_data,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        handle: String,
        slot: Slot<XchandlesSlotValue>,
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

        let slot = registry.actual_slot(slot);
        let action_solution = ctx.alloc(&XchandlesExtendActionSolution {
            pricing_puzzle_reveal,
            pricing_solution: XchandlesPricingSolution {
                buy_time,
                current_expiration: slot.info.value.expiration,
                handle: handle.clone(),
                num_periods,
            },
            cat_maker_puzzle_reveal,
            cat_maker_solution: (),
            neighbors: slot.info.value.neighbors,
            rest: slot.info.value.rest_data(),
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        let renew_amount =
            XchandlesFactorPricingPuzzleArgs::get_price(base_handle_price, &handle, num_periods);

        let notarized_payment = NotarizedPayment {
            nonce: clvm_tuple!(handle.clone(), slot.info.value.expiration)
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

        let mut extend_ann = clvm_tuple!(renew_amount, handle).tree_hash().to_vec();
        extend_ann.insert(0, b'e');

        Ok((
            Conditions::new()
                .assert_puzzle_announcement(announcement_id(registry.coin.puzzle_hash, extend_ann)),
            notarized_payment,
        ))
    }
}
