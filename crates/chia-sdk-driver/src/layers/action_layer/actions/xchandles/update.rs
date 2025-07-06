use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::{Bytes, Bytes32},
};
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_wallet_sdk::{
    driver::{DriverError, Spend, SpendContext},
    types::Conditions,
};
use clvm_traits::{clvm_tuple, FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{
    Action, Slot, SpendContextExt, XchandlesConstants, XchandlesDataValue, XchandlesRegistry,
    XchandlesSlotValue,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesUpdateAction {
    pub launcher_id: Bytes32,
}

impl ToTreeHash for XchandlesUpdateAction {
    fn tree_hash(&self) -> TreeHash {
        XchandlesUpdateActionArgs::curry_tree_hash(self.launcher_id)
    }
}

impl Action<XchandlesRegistry> for XchandlesUpdateAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
        }
    }
}

impl XchandlesUpdateAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        Ok(CurriedProgram {
            program: ctx.xchandles_update_puzzle()?,
            args: XchandlesUpdateActionArgs::new(self.launcher_id),
        }
        .to_clvm(ctx)?)
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesSlotValue, DriverError> {
        let solution = ctx.extract::<XchandlesUpdateActionSolution>(solution)?;

        Ok(solution.current_slot_value)
    }

    pub fn created_slot_value(
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesSlotValue, DriverError> {
        let solution = ctx.extract::<XchandlesUpdateActionSolution>(solution)?;

        Ok(solution.current_slot_value.with_data(
            solution.new_data.owner_launcher_id,
            solution.new_data.resolved_data,
        ))
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        slot: Slot<XchandlesSlotValue>,
        new_owner_launcher_id: Bytes32,
        new_resolved_data: Bytes,
        announcer_inner_puzzle_hash: Bytes32,
    ) -> Result<Conditions, DriverError> {
        // spend self
        let slot = registry.actual_slot(slot);
        let action_solution = ctx.alloc(&XchandlesUpdateActionSolution {
            current_slot_value: slot.info.value.clone(),
            new_data: XchandlesDataValue {
                owner_launcher_id: new_owner_launcher_id,
                resolved_data: new_resolved_data.clone(),
            },
            announcer_inner_puzzle_hash,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slot
        let my_inner_puzzle_hash: Bytes32 = registry.info.inner_puzzle_hash().into();

        let msg: Bytes32 = clvm_tuple!(
            slot.info.value.handle_hash,
            clvm_tuple!(new_owner_launcher_id, new_resolved_data.clone())
        )
        .tree_hash()
        .into();

        slot.spend(ctx, my_inner_puzzle_hash)?;

        Ok(Conditions::new().send_message(
            18,
            msg.into(),
            vec![ctx.alloc(&registry.coin.puzzle_hash)?],
        ))
    }
}

pub const XCHANDLES_UPDATE_PUZZLE: [u8; 824] = hex!("ff02ffff01ff02ffff03ffff22ffff09ffff0dff82025f80ffff012080ffff15ffff0141ffff0dff82035f808080ffff01ff04ff2fffff04ffff04ff10ffff04ff82029fff808080ffff04ffff02ff3effff04ff02ffff04ff17ffff04ffff02ff2effff04ff02ffff04ff819fff80808080ff8080808080ffff04ffff02ff16ffff04ff02ffff04ff17ffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff82021fff82031f80ffff04ff82029fff82015f8080ff80808080ff8080808080ffff04ffff04ff14ffff04ffff0112ffff04ffff02ff2effff04ff02ffff04ffff04ff82021fff82015f80ff80808080ffff04ffff0bff5affff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12ff6affff02ff2effff04ff02ffff04ffff04ff05ffff04ff82059fff0b8080ff8080808080ffff0bff12ffff0bff7affff0bff12ffff0bff12ff6aff8201df80ffff0bff12ff6aff4a808080ff4a808080ff4a808080ff8080808080ff808080808080ffff01ff088080ff0180ffff04ffff01ffffff5533ff4342ffff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff04ff18ffff04ffff0bff5affff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12ff6affff0bffff0101ff0b8080ffff0bff12ff6aff4a808080ff4a808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04ff1cffff04ffff0112ffff04ff80ffff04ffff0bff5affff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12ff6affff0bffff0101ff0b8080ffff0bff12ff6aff4a808080ff4a808080ff8080808080ff018080");

pub const XCHANDLES_UPDATE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    66824757990b68234d4540b28ea8442bfdb2e875952222f002ea93cd6f8d93cb
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesUpdateActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_launcher_mod_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

impl XchandlesUpdateActionArgs {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_launcher_mod_hash: SINGLETON_LAUNCHER_HASH.into(),
            slot_1st_curry_hash: Slot::<()>::first_curry_hash(launcher_id, 0).into(),
        }
    }
}

impl XchandlesUpdateActionArgs {
    pub fn curry_tree_hash(launcher_id: Bytes32) -> TreeHash {
        CurriedProgram {
            program: XCHANDLES_UPDATE_PUZZLE_HASH,
            args: XchandlesUpdateActionArgs::new(launcher_id),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesUpdateActionSolution {
    pub current_slot_value: XchandlesSlotValue,
    pub new_data: XchandlesDataValue,
    #[clvm(rest)]
    pub announcer_inner_puzzle_hash: Bytes32,
}
