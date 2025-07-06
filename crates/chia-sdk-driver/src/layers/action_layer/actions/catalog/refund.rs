use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
    puzzles::singleton::SingletonStruct,
};
use chia_wallet_sdk::{
    driver::{DriverError, Spend, SpendContext},
    types::{announcement_id, Conditions},
};
use clvm_traits::{clvm_tuple, FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{
    Action, CatalogPrecommitValue, CatalogRegistry, CatalogRegistryConstants, CatalogSlotValue,
    DefaultCatMakerArgs, PrecommitCoin, PrecommitLayer, Slot, SlotNeigborsInfo, SpendContextExt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRefundAction {
    pub launcher_id: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for CatalogRefundAction {
    fn tree_hash(&self) -> TreeHash {
        CatalogRefundActionArgs::curry_tree_hash(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        )
    }
}

impl Action<CatalogRegistry> for CatalogRefundAction {
    fn from_constants(constants: &CatalogRegistryConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            relative_block_height: constants.relative_block_height,
            payout_puzzle_hash: constants.precommit_payout_puzzle_hash,
        }
    }
}

impl CatalogRefundAction {
    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        Ok(CurriedProgram {
            program: ctx.catalog_refund_action_puzzle()?,
            args: CatalogRefundActionArgs::new(
                self.launcher_id,
                self.relative_block_height,
                self.payout_puzzle_hash,
            ),
        }
        .to_clvm(ctx)?)
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

    #[allow(clippy::too_many_arguments)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        catalog: &mut CatalogRegistry,
        tail_hash: Bytes32,
        neighbors: Option<SlotNeigborsInfo>,
        precommit_coin: PrecommitCoin<CatalogPrecommitValue>,
        slot: Option<Slot<CatalogSlotValue>>,
    ) -> Result<Conditions, DriverError> {
        // calculate announcement
        let refund_announcement: Bytes32 =
            clvm_tuple!(tail_hash, precommit_coin.value.initial_inner_puzzle_hash)
                .tree_hash()
                .into();
        let mut refund_announcement: Vec<u8> = refund_announcement.to_vec();
        refund_announcement.insert(0, b'$');

        let secure_conditions = Conditions::new().assert_puzzle_announcement(announcement_id(
            catalog.coin.puzzle_hash,
            refund_announcement,
        ));

        // spend precommit coin
        let spender_inner_puzzle_hash: Bytes32 = catalog.info.inner_puzzle_hash().into();
        let initial_inner_puzzle_hash = precommit_coin.value.initial_inner_puzzle_hash;
        precommit_coin.spend(
            ctx,
            0, // mode 0 = refund
            spender_inner_puzzle_hash,
        )?;

        // if there's a slot, spend it
        if let Some(slot) = slot {
            let slot = catalog.actual_slot(slot);
            slot.spend(ctx, spender_inner_puzzle_hash)?;
        }

        // then, create action spend
        let action_solution = CatalogRefundActionSolution {
            precommited_cat_maker_reveal: DefaultCatMakerArgs::get_puzzle(
                ctx,
                precommit_coin.asset_id.tree_hash().into(),
            )?,
            precommited_cat_maker_hash: DefaultCatMakerArgs::curry_tree_hash(
                precommit_coin.asset_id.tree_hash().into(),
            )
            .into(),
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

pub const CATALOG_REFUND_PUZZLE: [u8; 922] = hex!("ff02ffff01ff02ffff03ffff09ff4fffff02ff2effff04ff02ffff04ff81afff8080808080ffff01ff04ff17ffff02ff16ffff04ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ffff04ff8202efff821fef80ff80808080ffff04ffff22ffff09ff77ff8217ef80ffff09ff4fff578080ffff04ffff04ffff04ff28ffff04ffff0effff0124ffff02ff2effff04ff02ffff04ffff04ff8202efff8205ef80ff8080808080ff808080ffff04ffff04ff38ffff04ffff0113ffff04ff80ffff04ffff02ff81afffff04ffff02ff2affff04ff02ffff04ff05ffff04ff820befffff04ffff0bffff0102ff8202efffff0bffff0101ffff02ff2effff04ff02ffff04ffff04ff8205efffff04ff4fff82016f8080ff808080808080ff808080808080ff82016f8080ffff04ff8217efff808080808080ff808080ff8080808080808080ffff01ff088080ff0180ffff04ffff01ffffff33ff3e42ff02ffff02ffff03ff05ffff01ff0bff81fcffff02ff3affff04ff02ffff04ff09ffff04ffff02ff2cffff04ff02ffff04ff0dff80808080ff808080808080ffff0181dc80ff0180ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffffff04ff10ffff04ffff02ff2affff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ffff0bff81bcffff02ff3affff04ff02ffff04ff05ffff04ffff02ff2cffff04ff02ffff04ff07ff80808080ff808080808080ff0bff14ffff0bff14ff81dcff0580ffff0bff14ff0bff819c8080ffff02ffff03ff17ffff01ff04ffff02ff3effff04ff02ffff04ff05ffff04ff0bff8080808080ffff04ffff02ff12ffff04ff02ffff04ff05ffff04ff0bff8080808080ff2f8080ffff012f80ff0180ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04ff38ffff04ffff0112ffff04ff80ffff04ffff02ff2affff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ff8080808080ff018080");

pub const CATALOG_REFUND_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    3d4aefac7d53b8d36802d5e03aa4a12301fc7eadab60a497311fe7995c2ebf32
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct CatalogRefundActionArgs {
    pub precommit_1st_curry_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

impl CatalogRefundActionArgs {
    pub fn new(
        launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            precommit_1st_curry_hash: PrecommitLayer::<()>::first_curry_hash(
                SingletonStruct::new(launcher_id).tree_hash().into(),
                relative_block_height,
                payout_puzzle_hash,
            )
            .into(),
            slot_1st_curry_hash: Slot::<CatalogSlotValue>::first_curry_hash(launcher_id, 0).into(),
        }
    }
}

impl CatalogRefundActionArgs {
    pub fn curry_tree_hash(
        launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> TreeHash {
        CurriedProgram {
            program: CATALOG_REFUND_PUZZLE_HASH,
            args: CatalogRefundActionArgs::new(
                launcher_id,
                relative_block_height,
                payout_puzzle_hash,
            ),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogRefundActionSolution<P, S> {
    pub precommited_cat_maker_hash: Bytes32,
    pub precommited_cat_maker_reveal: P,
    pub precommited_cat_maker_solution: S,
    pub tail_hash: Bytes32,
    pub initial_nft_owner_ph: Bytes32,
    pub refund_puzzle_hash_hash: Bytes32,
    pub precommit_amount: u64,
    #[clvm(rest)]
    pub neighbors: Option<SlotNeigborsInfo>,
}
