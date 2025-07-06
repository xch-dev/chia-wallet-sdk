use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
};
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{
    NFT_OWNERSHIP_LAYER_HASH, NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH,
    NFT_STATE_LAYER_HASH, SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH,
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
    DefaultCatMakerArgs, PrecommitCoin, PrecommitLayer, Slot, SpendContextExt,
    UniquenessPrelauncher,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRegisterAction {
    pub launcher_id: Bytes32,
    pub royalty_puzzle_hash_hash: Bytes32,
    pub trade_price_percentage: u16,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for CatalogRegisterAction {
    fn tree_hash(&self) -> TreeHash {
        CatalogRegisterActionArgs::curry_tree_hash(
            self.launcher_id,
            self.royalty_puzzle_hash_hash,
            self.trade_price_percentage,
            self.relative_block_height,
            self.payout_puzzle_hash,
        )
    }
}

impl Action<CatalogRegistry> for CatalogRegisterAction {
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
    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        Ok(CurriedProgram {
            program: ctx.catalog_register_action_puzzle()?,
            args: CatalogRegisterActionArgs::new(
                self.launcher_id,
                self.royalty_puzzle_hash_hash,
                self.trade_price_percentage,
                self.relative_block_height,
                self.payout_puzzle_hash,
            ),
        }
        .to_clvm(ctx)?)
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
        precommit_coin: PrecommitCoin<CatalogPrecommitValue>,
        eve_nft_inner_spend: Spend,
    ) -> Result<Conditions, DriverError> {
        // calculate announcement
        let register_announcement: Bytes32 =
            clvm_tuple!(tail_hash, precommit_coin.value.initial_inner_puzzle_hash)
                .tree_hash()
                .into();
        let mut register_announcement: Vec<u8> = register_announcement.to_vec();
        register_announcement.insert(0, b'r');

        // spend precommit coin
        let initial_inner_puzzle_hash = precommit_coin.value.initial_inner_puzzle_hash;
        let my_inner_puzzle_hash = catalog.info.inner_puzzle_hash().into();
        precommit_coin.spend(
            ctx,
            1, // mode 1 = register
            my_inner_puzzle_hash,
        )?;

        // spend uniqueness prelauncher
        let uniqueness_prelauncher =
            UniquenessPrelauncher::<Bytes32>::new(ctx, catalog.coin.coin_id(), tail_hash)?;
        let nft_launcher = uniqueness_prelauncher.spend(ctx)?;

        // launch eve nft
        let (_, nft) = nft_launcher.mint_eve_nft(
            ctx,
            initial_inner_puzzle_hash,
            (),
            ANY_METADATA_UPDATER_HASH.into(),
            catalog.info.constants.royalty_address,
            catalog.info.constants.royalty_basis_points,
        )?;

        // spend nft launcher
        let _new_nft = nft.spend(ctx, eve_nft_inner_spend)?;

        // finally, spend self
        let (left_slot, right_slot) = catalog.actual_neigbors(tail_hash, left_slot, right_slot);
        let my_solution = CatalogRegisterActionSolution {
            cat_maker_reveal: DefaultCatMakerArgs::get_puzzle(
                ctx,
                precommit_coin.asset_id.tree_hash().into(),
            )?,
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

pub const ANY_METADATA_UPDATER: [u8; 23] = hex!("ff04ffff04ff0bffff04ff05ff808080ffff01ff808080");

pub const ANY_METADATA_UPDATER_HASH: TreeHash = TreeHash::new(hex!(
    "
    9f28d55242a3bd2b3661c38ba8647392c26bb86594050ea6d33aad1725ca3eea
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct NftPack {
    pub launcher_hash: Bytes32,
    pub singleton_mod_hash: Bytes32,
    pub state_layer_mod_hash: Bytes32,
    pub metadata_updater_hash_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub transfer_program_mod_hash: Bytes32,
    pub royalty_puzzle_hash_hash: Bytes32,
    pub trade_price_percentage: u16,
}

impl NftPack {
    pub fn new(royalty_puzzle_hash_hash: Bytes32, trade_price_percentage: u16) -> Self {
        let meta_updater_hash: Bytes32 = ANY_METADATA_UPDATER_HASH.into();

        Self {
            launcher_hash: SINGLETON_LAUNCHER_HASH.into(),
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            metadata_updater_hash_hash: meta_updater_hash.tree_hash().into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            transfer_program_mod_hash:
                NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH.into(),
            royalty_puzzle_hash_hash,
            trade_price_percentage,
        }
    }
}

pub const CATALOG_REGISTER_PUZZLE: [u8; 1578] = hex!("ff02ffff01ff02ffff03ffff22ffff0aff8205bfff822fbf80ffff0aff82bfbfff8205bf80ffff09ffff02ff2effff04ff02ffff04ff82013fff80808080ff82015f8080ffff01ff04ff5fffff02ff32ffff04ff02ffff04ff05ffff04ff8301ffbfffff04ff820bbfffff04ffff02ff3affff04ff02ffff04ff0bffff04ffff0bffff0101ff8205bf80ff8080808080ffff04ffff04ffff04ff28ffff04ff8301ffbfff808080ffff04ffff04ff24ffff04ffff0effff0172ffff02ff2effff04ff02ffff04ffff04ff8205bfff820bbf80ff8080808080ff808080ffff04ffff02ff3effff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ff822fbfffff04ff825fbfff82bfbf8080ff80808080ff8080808080ffff04ffff02ff3effff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ff82bfbfffff04ff822fbfff83017fbf8080ff80808080ff8080808080ffff04ffff02ff2affff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ff8205bfffff04ff822fbfff82bfbf8080ff80808080ff8080808080ffff04ffff02ff2affff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ff822fbfffff04ff825fbfff8205bf8080ff80808080ff8080808080ffff04ffff02ff2affff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ff82bfbfffff04ff8205bfff83017fbf8080ff80808080ff8080808080ffff04ffff04ff34ffff04ffff0113ffff04ffff0101ffff04ffff02ff82013fffff04ffff02ff3affff04ff02ffff04ff17ffff04ff8217bfffff04ffff0bffff0102ff8205bfffff0bffff0101ffff02ff2effff04ff02ffff04ffff04ff820bbfffff04ff82015fff8202bf8080ff808080808080ff808080808080ff8202bf8080ffff04ff8201dfff808080808080ff808080808080808080ff808080808080808080ffff01ff088080ff0180ffff04ffff01ffffff40ff4633ffff3e42ff02ff02ffff03ff05ffff01ff0bff81e2ffff02ff26ffff04ff02ffff04ff09ffff04ffff02ff3cffff04ff02ffff04ff0dff80808080ff808080808080ffff0181c280ff0180ffffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ffff04ff38ffff04ff2fffff01ff80808080ffff04ffff02ff36ffff04ff02ffff04ff05ffff04ff17ffff04ffff30ffff30ff0bff2fff8080ff09ffff010180ff808080808080ff5f8080ffff04ff38ffff04ffff02ff3affff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ff0bff81a2ffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff3cffff04ff02ffff04ff07ff80808080ff808080808080ffffff0bff2cffff0bff2cff81c2ff0580ffff0bff2cff0bff81828080ff04ff10ffff04ffff30ff17ffff02ff3affff04ff02ffff04ff15ffff04ffff02ff2effff04ff02ffff04ffff04ff15ffff04ff17ff098080ff80808080ffff04ffff02ff3affff04ff02ffff04ff2dffff04ffff0bffff0101ff2d80ffff04ff8182ffff04ff5dffff04ffff02ff3affff04ff02ffff04ff81bdffff04ffff0bffff0101ff81bd80ffff04ff8182ffff04ffff02ff3affff04ff02ffff04ff82017dffff04ffff02ff2effff04ff02ffff04ffff04ff15ffff04ff17ff098080ff80808080ffff04ff8202fdffff04ffff0bffff0101ff8205fd80ff80808080808080ffff04ff0bff8080808080808080ff8080808080808080ff808080808080ffff010180ff808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04ff34ffff04ffff0112ffff04ff80ffff04ffff02ff3affff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ff8080808080ff018080");

pub const CATALOG_REGISTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    028d83ae6f75c1a1fa40ebc68efb7c983257d0cc3fc7161a3418f63cca934e20
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct CatalogRegisterActionArgs {
    pub nft_pack: NftPack,
    pub uniqueness_prelauncher_1st_curry_hash: Bytes32,
    pub precommit_1st_curry_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

impl CatalogRegisterActionArgs {
    pub fn new(
        launcher_id: Bytes32,
        royalty_puzzle_hash_hash: Bytes32,
        trade_price_percentage: u16,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
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
}

impl CatalogRegisterActionArgs {
    pub fn curry_tree_hash(
        launcher_id: Bytes32,
        royalty_puzzle_hash_hash: Bytes32,
        trade_price_percentage: u16,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> TreeHash {
        CurriedProgram {
            program: CATALOG_REGISTER_PUZZLE_HASH,
            args: CatalogRegisterActionArgs::new(
                launcher_id,
                royalty_puzzle_hash_hash,
                trade_price_percentage,
                relative_block_height,
                payout_puzzle_hash,
            ),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogRegisterActionSolution<P, S> {
    pub cat_maker_reveal: P,
    pub cat_maker_solution: S,
    pub tail_hash: Bytes32,
    pub initial_nft_owner_ph: Bytes32,
    pub refund_puzzle_hash_hash: Bytes32,
    pub left_tail_hash: Bytes32,
    pub left_left_tail_hash: Bytes32,
    pub right_tail_hash: Bytes32,
    pub right_right_tail_hash: Bytes32,
    #[clvm(rest)]
    pub my_id: Bytes32,
}
