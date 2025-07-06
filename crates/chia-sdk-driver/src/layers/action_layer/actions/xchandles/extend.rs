use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
    puzzles::offer::{NotarizedPayment, Payment},
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_wallet_sdk::{
    driver::{DriverError, Spend, SpendContext},
    types::{announcement_id, Conditions},
};
use clvm_traits::{clvm_tuple, FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{
    Action, DefaultCatMakerArgs, Slot, SlotNeigborsInfo, SpendContextExt, XchandlesConstants,
    XchandlesDataValue, XchandlesRegistry, XchandlesSlotValue,
};

use super::{XchandlesFactorPricingPuzzleArgs, XchandlesPricingSolution};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesExtendAction {
    pub launcher_id: Bytes32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for XchandlesExtendAction {
    fn tree_hash(&self) -> TreeHash {
        XchandlesExtendActionArgs::curry_tree_hash(self.launcher_id, self.payout_puzzle_hash)
    }
}

impl Action<XchandlesRegistry> for XchandlesExtendAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            payout_puzzle_hash: constants.precommit_payout_puzzle_hash,
        }
    }
}

impl XchandlesExtendAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        Ok(CurriedProgram {
            program: ctx.xchandles_extend_puzzle()?,
            args: XchandlesExtendActionArgs::new(self.launcher_id, self.payout_puzzle_hash),
        }
        .to_clvm(ctx)?)
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
        let spender_inner_puzzle_hash: Bytes32 = registry.info.inner_puzzle_hash().into();

        // spend self
        let cat_maker_puzzle_reveal =
            DefaultCatMakerArgs::get_puzzle(ctx, payment_asset_id.tree_hash().into())?;
        let pricing_puzzle_reveal = XchandlesFactorPricingPuzzleArgs::get_puzzle(
            ctx,
            base_handle_price,
            registration_period,
        )?;

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

        let mut extend_ann: Vec<u8> = clvm_tuple!(renew_amount, handle).tree_hash().to_vec();
        extend_ann.insert(0, b'e');

        Ok((
            Conditions::new()
                .assert_puzzle_announcement(announcement_id(registry.coin.puzzle_hash, extend_ann)),
            notarized_payment,
        ))
    }
}

pub const XCHANDLES_EXTEND_PUZZLE: [u8; 964] = hex!("ff02ffff01ff02ffff03ffff22ffff09ff81afffff02ff2effff04ff02ffff04ff8202dfff8080808080ffff09ff82016fffff02ff2effff04ff02ffff04ff819fff808080808080ffff01ff04ff2fffff04ffff02ff3effff04ff02ffff04ff17ffff04ffff02ff2effff04ff02ffff04ffff04ffff04ffff0bffff0101ff820b5f80ff820bdf80ffff04ff82055fff820fdf8080ff80808080ff8080808080ffff04ffff04ff3cffff04ffff0effff0165ffff02ff2effff04ff02ffff04ffff04ffff05ffff02ff819fff82015f8080ff820b5f80ff8080808080ff808080ffff04ffff04ff10ffff04ff82055fff808080ffff04ffff04ff14ffff04ff82025fff808080ffff04ffff02ff16ffff04ff02ffff04ff17ffff04ffff02ff2effff04ff02ffff04ffff04ffff04ffff0bffff0101ff820b5f80ff820bdf80ffff04ffff10ff82055fffff06ffff02ff819fff82015f808080ff820fdf8080ff80808080ff8080808080ffff04ffff04ff18ffff04ffff0bffff02ff8202dfffff04ff05ff8205df8080ffff02ff2effff04ff02ffff04ffff04ffff02ff2effff04ff02ffff04ffff04ff820b5fff82055f80ff80808080ffff04ffff04ff0bffff04ffff05ffff02ff819fff82015f8080ffff04ffff04ff0bff8080ff80808080ff808080ff8080808080ff808080ff8080808080808080ffff01ff088080ff0180ffff04ffff01ffffff553fff51ff333effff42ff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff04ff2cffff04ffff0bff81baffff0bff2affff0bff2aff81daff0580ffff0bff2affff0bff81faffff0bff2affff0bff2aff81daffff0bffff0101ff0b8080ffff0bff2aff81daff819a808080ff819a808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04ff12ffff04ffff0112ffff04ff80ffff04ffff0bff81baffff0bff2affff0bff2aff81daff0580ffff0bff2affff0bff81faffff0bff2affff0bff2aff81daffff0bffff0101ff0b8080ffff0bff2aff81daff819a808080ff819a808080ff8080808080ff018080");

pub const XCHANDLES_EXTEND_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    caa665c939f3de5d90dd22b00d092ba7c794300bf994b9ddcea536fa77843e08
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExtendActionArgs {
    pub offer_mod_hash: Bytes32,
    pub payout_puzzle_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

impl XchandlesExtendActionArgs {
    pub fn new(launcher_id: Bytes32, payout_puzzle_hash: Bytes32) -> Self {
        Self {
            offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
            payout_puzzle_hash,
            slot_1st_curry_hash: Slot::<()>::first_curry_hash(launcher_id, 0).into(),
        }
    }
}

impl XchandlesExtendActionArgs {
    pub fn curry_tree_hash(launcher_id: Bytes32, payout_puzzle_hash: Bytes32) -> TreeHash {
        CurriedProgram {
            program: XCHANDLES_EXTEND_PUZZLE_HASH,
            args: XchandlesExtendActionArgs::new(launcher_id, payout_puzzle_hash),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesExtendActionSolution<PP, PS, CMP, CMS> {
    pub pricing_puzzle_reveal: PP,
    pub pricing_solution: PS,
    pub cat_maker_puzzle_reveal: CMP,
    pub cat_maker_solution: CMS,
    pub neighbors: SlotNeigborsInfo,
    #[clvm(rest)]
    pub rest: XchandlesDataValue,
}
