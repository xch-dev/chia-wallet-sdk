use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        CatalogRefundActionArgs, CatalogRefundActionSolution, CatalogSlotValue,
        DefaultCatMakerArgs, PrecommitSpendMode, SlotNeigborsInfo,
    },
    Conditions, Mod,
};
use clvm_traits::{clvm_tuple, FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    CatalogPrecommitValue, CatalogRegistry, CatalogRegistryConstants, DriverError, PrecommitCoin,
    PrecommitLayer, SingletonAction, Slot, Spend, SpendContext,
};

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

    pub fn spent_slot_value(
        &self,
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<Option<CatalogSlotValue>, DriverError> {
        let params = CatalogRefundActionSolution::<NodePtr, ()>::from_clvm(ctx, solution)?;

        Ok(params.neighbors.map(|neighbors| CatalogSlotValue {
            asset_id: params.tail_hash,
            neighbors,
        }))
    }

    pub fn created_slot_value(
        &self,
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<Option<CatalogSlotValue>, DriverError> {
        self.spent_slot_value(ctx, solution)
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
        let mut refund_announcement =
            clvm_tuple!(tail_hash, precommit_coin.value.initial_inner_puzzle_hash)
                .tree_hash()
                .to_vec();
        refund_announcement.insert(0, b'$');

        let secure_conditions = Conditions::new().assert_puzzle_announcement(announcement_id(
            catalog.coin.puzzle_hash,
            refund_announcement,
        ));

        // spend precommit coin
        let spender_inner_puzzle_hash = catalog.info.inner_puzzle_hash().into();
        let initial_inner_puzzle_hash = precommit_coin.value.initial_inner_puzzle_hash;
        precommit_coin.spend(ctx, PrecommitSpendMode::REFUND, spender_inner_puzzle_hash)?;

        // if there's a slot, spend it
        if let Some(slot) = slot {
            let slot = catalog.actual_slot(slot);
            slot.spend(ctx, spender_inner_puzzle_hash)?;
        }

        // then, create action spend
        let cat_maker_args = DefaultCatMakerArgs::new(precommit_coin.asset_id.tree_hash().into());
        let action_solution = CatalogRefundActionSolution {
            precommited_cat_maker_reveal: ctx.curry(cat_maker_args)?,
            precommited_cat_maker_hash: cat_maker_args.curry_tree_hash().into(),
            precommited_cat_maker_solution: (),
            tail_hash,
            initial_nft_owner_ph: initial_inner_puzzle_hash,
            refund_puzzle_hash_hash: precommit_coin.refund_puzzle_hash.tree_hash().into(),
            precommit_amount: precommit_coin.coin.amount,
            neighbors,
        };
        let action_solution = action_solution.to_clvm(ctx)?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        catalog.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok(secure_conditions)
    }
}
