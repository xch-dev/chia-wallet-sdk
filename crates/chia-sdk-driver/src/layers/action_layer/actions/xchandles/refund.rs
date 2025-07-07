use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        XchandlesRefundActionArgs, XchandlesRefundActionSolution, XchandlesSlotValue,
        XCHANDLES_REFUND_PUZZLE_HASH,
    },
    Conditions,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DefaultCatMakerArgs, DriverError, PrecommitCoin, PrecommitLayer, SingletonAction, Slot, Spend,
    SpendContext, XchandlesConstants, XchandlesPrecommitValue, XchandlesRegistry,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesRefundAction {
    pub launcher_id: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for XchandlesRefundAction {
    fn tree_hash(&self) -> TreeHash {
        CurriedProgram {
            program: XCHANDLES_REFUND_PUZZLE_HASH,
            args: Self::new_args(
                self.launcher_id,
                self.relative_block_height,
                self.payout_puzzle_hash,
            ),
        }
        .tree_hash()
    }
}

impl SingletonAction<XchandlesRegistry> for XchandlesRefundAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            relative_block_height: constants.relative_block_height,
            payout_puzzle_hash: constants.precommit_payout_puzzle_hash,
        }
    }
}

impl XchandlesRefundAction {
    pub fn new_args(
        launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> XchandlesRefundActionArgs {
        XchandlesRefundActionArgs {
            precommit_1st_curry_hash: PrecommitLayer::<()>::first_curry_hash(
                SingletonStruct::new(launcher_id).tree_hash().into(),
                relative_block_height,
                payout_puzzle_hash,
            )
            .into(),
            slot_1st_curry_hash: Slot::<()>::first_curry_hash(launcher_id, 0).into(),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        ))
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<Option<XchandlesSlotValue>, DriverError> {
        let solution =
            XchandlesRefundActionSolution::<NodePtr, NodePtr, NodePtr, NodePtr, NodePtr>::from_clvm(
                ctx, solution,
            )?;

        Ok(solution.slot_value)
    }

    pub fn created_slot_value(
        spent_slot_value: Option<XchandlesSlotValue>,
    ) -> Option<XchandlesSlotValue> {
        spent_slot_value // nothing changed; just oracle
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        precommit_coin: PrecommitCoin<XchandlesPrecommitValue>,
        precommited_pricing_puzzle_reveal: NodePtr,
        precommited_pricing_puzzle_solution: NodePtr,
        slot: Option<Slot<XchandlesSlotValue>>,
    ) -> Result<Conditions, DriverError> {
        // calculate announcement
        let mut refund_announcement: Vec<u8> = precommit_coin.coin.puzzle_hash.to_vec();
        refund_announcement.insert(0, b'$');

        // spend precommit coin
        let my_inner_puzzle_hash: Bytes32 = registry.info.inner_puzzle_hash().into();
        precommit_coin.spend(
            ctx,
            0, // mode 0 = refund
            my_inner_puzzle_hash,
        )?;

        // spend self
        let slot = slot.map(|s| registry.actual_slot(s));
        let action_solution = XchandlesRefundActionSolution {
            precommited_cat_maker_reveal: DefaultCatMakerArgs::get_puzzle(
                ctx,
                precommit_coin.asset_id.tree_hash().into(),
            )?,
            precommited_cat_maker_hash: DefaultCatMakerArgs::curry_tree_hash(
                precommit_coin.asset_id.tree_hash().into(),
            )
            .into(),
            precommited_cat_maker_solution: (),
            precommited_pricing_puzzle_reveal,
            precommited_pricing_puzzle_hash: ctx
                .tree_hash(precommited_pricing_puzzle_reveal)
                .into(),
            precommited_pricing_puzzle_solution,
            handle: precommit_coin.value.handle.clone(),
            secret: precommit_coin.value.secret,
            precommited_owner_launcher_id: precommit_coin.value.owner_launcher_id,
            precommited_resolved_data: precommit_coin.value.resolved_data.clone(),
            refund_puzzle_hash_hash: precommit_coin.refund_puzzle_hash.tree_hash().into(),
            precommit_amount: precommit_coin.coin.amount,
            slot_value: slot.as_ref().map(|slot| slot.info.value.clone()),
        }
        .to_clvm(ctx)?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // if there's a slot, spend it
        if let Some(slot) = slot {
            slot.spend(ctx, my_inner_puzzle_hash)?;
        }

        Ok(
            Conditions::new().assert_puzzle_announcement(announcement_id(
                registry.coin.puzzle_hash,
                refund_announcement,
            )),
        )
    }
}
