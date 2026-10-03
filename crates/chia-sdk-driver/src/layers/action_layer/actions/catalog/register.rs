use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_types::{
    Conditions, Mod, announcement_id,
    puzzles::{
        ANY_METADATA_UPDATER_HASH, CatalogDoubleTailHashData, CatalogOtherPrecommitData,
        CatalogRegisterActionArgs, CatalogRegisterActionSolution, CatalogSlotValue,
        DefaultCatMakerArgs, NftPack, PrecommitSpendMode, PuzzleAndSolution,
    },
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    CatalogPrecommitValue, CatalogRegistry, CatalogRegistryConstants,
    CatalogRegistryCreatedAnnouncementPrefix, DriverError, HashedPtr, PrecommitCoin,
    PrecommitLayer, SingletonAction, Slot, Spend, SpendContext, UniquenessPrelauncher,
};

use super::CatalogRegisterActionLog;

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

    pub fn get_log(
        ctx: &SpendContext,
        solution: NodePtr,
        registration_price: u64,
    ) -> Result<CatalogRegisterActionLog, DriverError> {
        let params = CatalogRegisterActionSolution::<NodePtr, ()>::from_clvm(ctx, solution)?;

        let spent_left_slot = CatalogSlotValue::new(
            params.left_data.this_counter,
            params.left_data.this_tail_hash,
            params.left_data.this_this_tail_hash,
            params.right_data.this_tail_hash,
        );
        let spent_right_slot = CatalogSlotValue::new(
            params.right_data.this_counter,
            params.right_data.this_tail_hash,
            params.left_data.this_tail_hash,
            params.right_data.this_this_tail_hash,
        );

        let tail_hash = params.other_precommit_data.tail_hash;
        let created_left_slot = CatalogSlotValue::new(
            params.left_data.this_counter + 1,
            params.left_data.this_tail_hash,
            params.left_data.this_this_tail_hash,
            tail_hash,
        );
        let created_tail_slot = CatalogSlotValue::new(
            0,
            tail_hash,
            params.left_data.this_tail_hash,
            params.right_data.this_tail_hash,
        );
        let created_right_slot = CatalogSlotValue::new(
            params.right_data.this_counter + 1,
            params.right_data.this_tail_hash,
            tail_hash,
            params.right_data.this_this_tail_hash,
        );

        let prelauncher_full_puzzle_hash =
            UniquenessPrelauncher::<Bytes32>::puzzle_hash(tail_hash.tree_hash()).into();
        let prelauncher_id = Coin::new(params.my_id, prelauncher_full_puzzle_hash, 0).coin_id();
        let launcher_id = Coin::new(prelauncher_id, SINGLETON_LAUNCHER_HASH.into(), 1).coin_id();

        Ok(CatalogRegisterActionLog {
            spent_left_slot,
            spent_right_slot,
            created_left_slot,
            created_tail_slot,
            created_right_slot,
            prelauncher_full_puzzle_hash,
            prelauncher_id,
            launcher_id,
            registered_tail_hash: tail_hash,
            registered_initial_inner_puzzle_hash: params.other_precommit_data.initial_nft_owner_ph,
            precommit_amount: registration_price,
        })
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
        let register_announcement = CatalogRegistryCreatedAnnouncementPrefix::register(
            tail_hash,
            precommit_coin.value.initial_inner_puzzle_hash,
        );

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
            precommitted_cat_maker_data: PuzzleAndSolution::new(
                ctx.curry(DefaultCatMakerArgs::new(
                    precommit_coin.asset_id.tree_hash().into(),
                ))?,
                (),
            ),
            other_precommit_data: CatalogOtherPrecommitData::new(
                tail_hash,
                initial_inner_puzzle_hash,
                precommit_coin.refund_puzzle_hash.tree_hash().into(),
            ),
            left_data: CatalogDoubleTailHashData::new(
                left_slot.info.value.counter,
                left_slot.info.value.asset_id,
                left_slot.info.value.neighbors.left_value,
            ),
            right_data: CatalogDoubleTailHashData::new(
                right_slot.info.value.counter,
                right_slot.info.value.asset_id,
                right_slot.info.value.neighbors.right_value,
            ),
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
