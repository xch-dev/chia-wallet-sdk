use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_sdk_types::{
    Conditions, Mod, announcement_id,
    puzzles::{
        DefaultCatMakerArgs, PrecommitSpendMode, PuzzleHashPuzzleAndSolution,
        XchandlesHandleSlotValue, XchandlesOtherPrecommitData, XchandlesPricingSolution,
        XchandlesRefundActionArgs, XchandlesRefundActionSolution, XchandlesSlotNonce,
    },
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, PrecommitCoin, PrecommitLayer, SingletonAction, Slot, Spend, SpendContext,
    XchandlesConstants, XchandlesPrecommitValue, XchandlesRegistry,
    XchandlesRegistryCreatedAnnouncementPrefix,
};

use super::{XchandlesPrecommitValueLog, XchandlesRefundActionLog, run_pricing_output};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesRefundAction {
    pub launcher_id: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for XchandlesRefundAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        )
        .curry_tree_hash()
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
            handle_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                XchandlesSlotNonce::HANDLE.to_u64(),
            )
            .into(),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        ))
    }

    pub fn get_log(
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesRefundActionLog, DriverError> {
        let solution =
            XchandlesRefundActionSolution::<NodePtr, (), NodePtr, NodePtr, Bytes32>::from_clvm(
                ctx, solution,
            )?;

        let pricing_solution = ctx.extract::<XchandlesPricingSolution>(
            solution.precommited_pricing_puzzle_and_solution.solution,
        )?;

        let (precommitted_total_price, precommitted_registered_time) = run_pricing_output(
            ctx,
            solution.precommited_pricing_puzzle_and_solution.puzzle,
            solution.precommited_pricing_puzzle_and_solution.solution,
        )?;

        let handle = solution.handle.clone();
        let precommit_value = XchandlesPrecommitValueLog::new(
            solution.precommited_cat_maker_and_solution.puzzle_hash,
            (),
            solution.precommited_pricing_puzzle_and_solution.puzzle_hash,
            pricing_solution,
            handle,
            solution.other_precommit_data.refund_and_secret.secret,
            solution.other_precommit_data.launcher_ids.owner_launcher_id,
            solution
                .other_precommit_data
                .launcher_ids
                .resolved_launcher_id,
        );

        let spent_slot = solution.slot_value;
        let created_slot = spent_slot.map(|slot| slot.with_counter(slot.counter + 1));

        Ok(XchandlesRefundActionLog {
            spent_slot,
            created_slot,
            precommit_value,
            precommitted_total_price,
            precommitted_registered_time,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        precommit_coin: &PrecommitCoin<XchandlesPrecommitValue>,
        precommited_pricing_puzzle_reveal: NodePtr,
        precommited_pricing_puzzle_solution: NodePtr,
        slot: Option<Slot<XchandlesHandleSlotValue>>,
    ) -> Result<Conditions, DriverError> {
        // calculate announcement
        let refund_announcement =
            XchandlesRegistryCreatedAnnouncementPrefix::refund(precommit_coin.coin.puzzle_hash);

        // spend precommit coin
        let my_inner_puzzle_hash = registry.info.inner_puzzle_hash().into();
        precommit_coin.spend(ctx, PrecommitSpendMode::REFUND, my_inner_puzzle_hash)?;

        // spend self
        let slot = slot.map(|s| registry.actual_handle_slot(s));
        let cat_maker_args = DefaultCatMakerArgs::new(precommit_coin.asset_id.tree_hash().into());
        let action_solution = XchandlesRefundActionSolution {
            precommited_pricing_puzzle_and_solution: PuzzleHashPuzzleAndSolution::new(
                ctx.tree_hash(precommited_pricing_puzzle_reveal).into(),
                precommited_pricing_puzzle_reveal,
                precommited_pricing_puzzle_solution,
            ),
            precommited_cat_maker_and_solution: PuzzleHashPuzzleAndSolution::new(
                cat_maker_args.curry_tree_hash().into(),
                ctx.curry(cat_maker_args)?,
                (),
            ),
            handle: precommit_coin.value.handle.clone(),
            precommit_amount: precommit_coin.coin.amount,
            slot_value: slot.as_ref().map(|slot| slot.info.value),
            other_precommit_data: XchandlesOtherPrecommitData::new(
                precommit_coin.value.owner_launcher_id,
                precommit_coin.value.resolved_launcher_id,
                precommit_coin.refund_puzzle_hash.tree_hash().into(),
                precommit_coin.value.secret,
            ),
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
