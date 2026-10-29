use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_sdk_types::{
    Conditions, Mod, announcement_id,
    puzzles::{
        CatalogOtherPrecommitData, CatalogRefundActionArgs, CatalogRefundActionSolution,
        CatalogSlotValue, DefaultCatMakerArgs, PrecommitSpendMode, PuzzleAndSolution,
        SlotNeigborsInfo,
    },
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    CatalogPrecommitValue, CatalogRegistry, CatalogRegistryConstants,
    CatalogRegistryCreatedAnnouncementPrefix, CatalogRegistryState, DriverError, PrecommitCoin,
    PrecommitLayer, SingletonAction, Slot, Spend, SpendContext,
};

use super::CatalogRefundActionLog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogRefundAction {
    pub launcher_id: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for CatalogRefundAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        )
        .curry_tree_hash()
    }
}

impl SingletonAction<CatalogRegistry> for CatalogRefundAction {
    fn from_constants(constants: &CatalogRegistryConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            relative_block_height: constants.relative_block_height,
            payout_puzzle_hash: constants.precommit_payout_puzzle_hash,
        }
    }
}

impl CatalogRefundAction {
    pub fn new_args(
        launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> CatalogRefundActionArgs {
        CatalogRefundActionArgs {
            precommit_1st_curry_hash: PrecommitLayer::<()>::first_curry_hash(
                SingletonStruct::new(launcher_id).tree_hash().into(),
                relative_block_height,
                payout_puzzle_hash,
            )
            .into(),
            slot_1st_curry_hash: Slot::<CatalogSlotValue>::first_curry_hash(launcher_id, 0).into(),
        }
    }

    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        ))
    }

    pub fn get_log(
        ctx: &mut SpendContext,
        solution: NodePtr,
        state: CatalogRegistryState,
    ) -> Result<CatalogRefundActionLog, DriverError> {
        let params = CatalogRefundActionSolution::<NodePtr, ()>::from_clvm(ctx, solution)?;

        let cat_maker_hash = ctx
            .tree_hash(params.precommited_cat_maker_and_solution.puzzle)
            .into();

        let slots_spent = state.registration_price == params.precommit_amount
            && state.cat_maker_puzzle_hash == cat_maker_hash
            && params.neighbors.is_some();

        let spent_slot = if slots_spent {
            Some(CatalogSlotValue {
                counter: params.slot_counter,
                asset_id: params.other_precommit_data.tail_hash,
                neighbors: params.neighbors.unwrap(),
            })
        } else {
            None
        };

        let created_slot = spent_slot.map(|mut slot| {
            slot.counter += 1;
            slot
        });

        Ok(CatalogRefundActionLog {
            spent_slot,
            created_slot,
            registered_tail_hash: params.other_precommit_data.tail_hash,
            registered_initial_inner_puzzle_hash: params.other_precommit_data.initial_nft_owner_ph,
            precommit_amount: params.precommit_amount,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        catalog: &mut CatalogRegistry,
        tail_hash: Bytes32,
        neighbors: Option<SlotNeigborsInfo>,
        precommit_coin: &PrecommitCoin<CatalogPrecommitValue>,
        slot: Option<Slot<CatalogSlotValue>>,
    ) -> Result<Conditions, DriverError> {
        // calculate announcement
        let refund_announcement = CatalogRegistryCreatedAnnouncementPrefix::refund(
            tail_hash,
            precommit_coin.value.initial_inner_puzzle_hash,
        );

        let secure_conditions = Conditions::new().assert_puzzle_announcement(announcement_id(
            catalog.coin.puzzle_hash,
            refund_announcement,
        ));

        // spend precommit coin
        let spender_inner_puzzle_hash = catalog.info.inner_puzzle_hash().into();
        let initial_inner_puzzle_hash = precommit_coin.value.initial_inner_puzzle_hash;
        precommit_coin.spend(ctx, PrecommitSpendMode::REFUND, spender_inner_puzzle_hash)?;

        // if there's a slot, spend it
        let counter = if let Some(slot) = slot {
            let slot = catalog.actual_slot(slot);
            let c = slot.info.value.counter;
            slot.spend(ctx, spender_inner_puzzle_hash)?;

            c
        } else {
            0
        };

        // then, create action spend
        let cat_maker_args = DefaultCatMakerArgs::new(precommit_coin.asset_id.tree_hash().into());
        let action_solution = CatalogRefundActionSolution {
            // precommited_cat_maker_and_solution: PuzzleHashPuzzleAndSolution::new(
            //     cat_maker_args.curry_tree_hash().into(),
            //     ctx.curry(cat_maker_args)?,
            //     (),
            // ),
            precommited_cat_maker_and_solution: PuzzleAndSolution::new(
                ctx.curry(cat_maker_args)?,
                (),
            ),
            other_precommit_data: CatalogOtherPrecommitData::new(
                tail_hash,
                initial_inner_puzzle_hash,
                precommit_coin.refund_puzzle_hash.tree_hash().into(),
            ),
            precommit_amount: precommit_coin.coin.amount,
            neighbors,
            slot_counter: counter,
        };
        let action_solution = action_solution.to_clvm(ctx)?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        catalog.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok(secure_conditions)
    }
}
