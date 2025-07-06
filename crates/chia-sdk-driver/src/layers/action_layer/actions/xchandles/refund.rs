use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::{Bytes, Bytes32},
    puzzles::singleton::SingletonStruct,
};
use chia_wallet_sdk::{
    driver::{DriverError, Spend, SpendContext},
    types::{announcement_id, Conditions},
};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{
    Action, DefaultCatMakerArgs, PrecommitCoin, PrecommitLayer, Slot, SpendContextExt,
    XchandlesConstants, XchandlesPrecommitValue, XchandlesRegistry, XchandlesSlotValue,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesRefundAction {
    pub launcher_id: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for XchandlesRefundAction {
    fn tree_hash(&self) -> TreeHash {
        XchandlesRefundActionArgs::curry_tree_hash(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        )
    }
}

impl Action<XchandlesRegistry> for XchandlesRefundAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            relative_block_height: constants.relative_block_height,
            payout_puzzle_hash: constants.precommit_payout_puzzle_hash,
        }
    }
}

impl XchandlesRefundAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        Ok(CurriedProgram {
            program: ctx.xchandles_refund_puzzle()?,
            args: XchandlesRefundActionArgs::new(
                self.launcher_id,
                self.relative_block_height,
                self.payout_puzzle_hash,
            ),
        }
        .to_clvm(ctx)?)
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

pub const XCHANDLES_REFUND_PUZZLE: [u8; 1075] =
    hex!("ff02ffff01ff02ffff03ffff22ffff09ff81afffff02ff2effff04ff02ffff04ff4fff8080808080ffff09ff8205efffff02ff2effff04ff02ffff04ff8202efff8080808080ffff02ffff03ff8303ffefffff01ff09ff8309ffefffff0bffff0101ff8217ef8080ffff01ff010180ff018080ffff01ff04ff17ffff02ff16ffff04ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ff8303ffefff80808080ffff04ffff02ffff03ffff22ffff09ff81afff5780ffff09ff8217efff825bef80ffff21ffff09ff8205efff81b780ffff09ff8205efff81f78080ffff09ff8302ffefffff05ffff02ff8202efff820bef80808080ffff01830bffefff8080ff0180ffff04ffff02ff4fffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ff83017fef80ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ffff02ff2effff04ff02ffff04ffff04ffff04ffff04ff81afff82016f80ffff04ff8205efff820bef8080ffff04ffff04ff8217efff822fef80ffff04ff825fefff82bfef808080ff808080808080ffff0bff3cff62ff42808080ff42808080ff42808080ff82016f8080ffff04ff8302ffefff808080808080808080ffff01ff088080ff0180ffff04ffff01ffffff5533ff3eff4202ffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ff18ffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ff0b8080ffff0bff3cff62ff42808080ff42808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ffff04ffff04ff2cffff04ffff0113ffff04ff80ffff04ff2fffff04ff5fff808080808080ffff04ffff04ff14ffff04ffff0effff0124ff2f80ff808080ffff02ffff03ff17ffff01ff04ffff04ff10ffff04ff17ff808080ffff04ffff02ff3effff04ff02ffff04ff05ffff04ff0bff8080808080ffff04ffff02ff1affff04ff02ffff04ff05ffff04ff0bff8080808080ff80808080ff8080ff01808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04ff2cffff04ffff0112ffff04ff80ffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ff0b8080ffff0bff3cff62ff42808080ff42808080ff8080808080ff018080");

pub const XCHANDLES_REFUND_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    c1469c124abadf18b0deee827c57f5189bc81d0f59aa07e2290676d0000b20a1
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesRefundActionArgs {
    pub precommit_1st_curry_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

impl XchandlesRefundActionArgs {
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
            slot_1st_curry_hash: Slot::<()>::first_curry_hash(launcher_id, 0).into(),
        }
    }
}

impl XchandlesRefundActionArgs {
    pub fn curry_tree_hash(
        launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> TreeHash {
        CurriedProgram {
            program: XCHANDLES_REFUND_PUZZLE_HASH,
            args: XchandlesRefundActionArgs::new(
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
pub struct XchandlesRefundActionSolution<CMP, CMS, PP, PS, S> {
    pub precommited_cat_maker_reveal: CMP,
    pub precommited_cat_maker_hash: Bytes32,
    pub precommited_cat_maker_solution: CMS,
    pub precommited_pricing_puzzle_reveal: PP,
    pub precommited_pricing_puzzle_hash: Bytes32,
    pub precommited_pricing_puzzle_solution: PS,
    pub handle: String,
    pub secret: S,
    pub precommited_owner_launcher_id: Bytes32,
    pub precommited_resolved_data: Bytes,
    pub refund_puzzle_hash_hash: Bytes32,
    pub precommit_amount: u64,
    #[clvm(rest)]
    pub slot_value: Option<XchandlesSlotValue>,
}
