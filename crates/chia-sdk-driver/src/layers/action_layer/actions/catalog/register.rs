use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        CatalogRegisterActionArgs, CatalogRegisterActionSolution, CatalogSlotValue,
        DefaultCatMakerArgs, NftPack, PrecommitSpendMode, ANY_METADATA_UPDATER_HASH,
    },
    Conditions, Mod,
};
use clvm_traits::{clvm_tuple, FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    CatalogPrecommitValue, CatalogRegistry, CatalogRegistryConstants, DriverError, HashedPtr,
    PrecommitCoin, PrecommitLayer, SingletonAction, Slot, Spend, SpendContext,
    UniquenessPrelauncher,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogRegisterAction {
    pub launcher_id: Bytes32,
    pub royalty_puzzle_hash_hash: Bytes32,
    pub trade_price_percentage: u16,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for CatalogRegisterAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(
            self.launcher_id,
            self.royalty_puzzle_hash_hash,
            self.trade_price_percentage,
            self.relative_block_height,
            self.payout_puzzle_hash,
        )
        .curry_tree_hash()
    }
}

impl SingletonAction<CatalogRegistry> for CatalogRegisterAction {
    fn from_constants(constants: &CatalogRegistryConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            royalty_puzzle_hash_hash: constants.royalty_address.tree_hash().into(),
            trade_price_percentage: constants.royalty_basis_points,
            relative_block_height: constants.relative_block_height,
            payout_puzzle_hash: constants.precommit_payout_puzzle_hash,
        }
    }
}

impl CatalogRegisterAction {
    pub fn new_args(
        launcher_id: Bytes32,
        royalty_puzzle_hash_hash: Bytes32,
        trade_price_percentage: u16,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> CatalogRegisterActionArgs {
        CatalogRegisterActionArgs {
            nft_pack: NftPack::new(royalty_puzzle_hash_hash, trade_price_percentage),
            uniqueness_prelauncher_1st_curry_hash: UniquenessPrelauncher::<()>::first_curry_hash()
                .into(),
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
            self.royalty_puzzle_hash_hash,
            self.trade_price_percentage,
            self.relative_block_height,
            self.payout_puzzle_hash,
        ))
    }

    pub fn spent_slot_values(
        &self,
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<[CatalogSlotValue; 2], DriverError> {
        let params = CatalogRegisterActionSolution::<NodePtr, ()>::from_clvm(ctx, solution)?;

        Ok([
            CatalogSlotValue::new(
                params.left_tail_hash,
                params.left_left_tail_hash,
                params.right_tail_hash,
            ),
            CatalogSlotValue::new(
                params.right_tail_hash,
                params.left_tail_hash,
                params.right_right_tail_hash,
            ),
        ])
    }

    pub fn created_slot_values(
        &self,
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<[CatalogSlotValue; 3], DriverError> {
        let params = CatalogRegisterActionSolution::<NodePtr, ()>::from_clvm(ctx, solution)?;

        Ok([
            CatalogSlotValue::new(
                params.left_tail_hash,
                params.left_left_tail_hash,
                params.tail_hash,
            ),
            CatalogSlotValue::new(
                params.tail_hash,
                params.left_tail_hash,
                params.right_tail_hash,
            ),
            CatalogSlotValue::new(
                params.right_tail_hash,
                params.tail_hash,
                params.right_right_tail_hash,
            ),
        ])
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        catalog: &mut CatalogRegistry,
        tail_hash: Bytes32,
        left_slot: Slot<CatalogSlotValue>,
        right_slot: Slot<CatalogSlotValue>,
        precommit_coin: &PrecommitCoin<CatalogPrecommitValue>,
        eve_nft_inner_spend: Spend,
    ) -> Result<Conditions, DriverError> {
        // calculate announcement
        let mut register_announcement =
            clvm_tuple!(tail_hash, precommit_coin.value.initial_inner_puzzle_hash)
                .tree_hash()
                .to_vec();
        register_announcement.insert(0, b'r');

        // spend precommit coin
        let initial_inner_puzzle_hash = precommit_coin.value.initial_inner_puzzle_hash;
        let my_inner_puzzle_hash = catalog.info.inner_puzzle_hash().into();
        precommit_coin.spend(ctx, PrecommitSpendMode::REGISTER, my_inner_puzzle_hash)?;

        // spend uniqueness prelauncher
        let uniqueness_prelauncher =
            UniquenessPrelauncher::<Bytes32>::new(ctx, catalog.coin.coin_id(), tail_hash)?;
        let nft_launcher = uniqueness_prelauncher.spend(ctx)?;

        // launch eve nft
        let (_, nft) = nft_launcher.mint_eve_nft(
            ctx,
            initial_inner_puzzle_hash,
            HashedPtr::NIL,
            ANY_METADATA_UPDATER_HASH.into(),
            catalog.info.constants.royalty_address,
            catalog.info.constants.royalty_basis_points,
        )?;

        // spend nft launcher
        let _new_nft = nft.spend(ctx, eve_nft_inner_spend)?;

        // finally, spend self
        let (left_slot, right_slot) = catalog.actual_neigbors(tail_hash, left_slot, right_slot);
        let my_solution = CatalogRegisterActionSolution {
            cat_maker_reveal: ctx.curry(DefaultCatMakerArgs::new(
                precommit_coin.asset_id.tree_hash().into(),
            ))?,
            cat_maker_solution: (),
            tail_hash,
            initial_nft_owner_ph: initial_inner_puzzle_hash,
            refund_puzzle_hash_hash: precommit_coin.refund_puzzle_hash.tree_hash().into(),
            left_tail_hash: left_slot.info.value.asset_id,
            left_left_tail_hash: left_slot.info.value.neighbors.left_value,
            right_tail_hash: right_slot.info.value.asset_id,
            right_right_tail_hash: right_slot.info.value.neighbors.right_value,
            my_id: catalog.coin.coin_id(),
        };
        let my_solution = my_solution.to_clvm(ctx)?;
        let my_puzzle = self.construct_puzzle(ctx)?;

        catalog.insert_action_spend(ctx, Spend::new(my_puzzle, my_solution))?;

        // spend slots
        left_slot.spend(ctx, my_inner_puzzle_hash)?;
        right_slot.spend(ctx, my_inner_puzzle_hash)?;

        Ok(
            Conditions::new().assert_puzzle_announcement(announcement_id(
                catalog.coin.puzzle_hash,
                register_announcement,
            )),
        )
    }
}
