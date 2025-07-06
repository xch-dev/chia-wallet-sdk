use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
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
    Action, DefaultCatMakerArgs, PrecommitCoin, PrecommitLayer, Slot, SlotNeigborsInfo,
    SpendContextExt, XchandlesConstants, XchandlesDataValue, XchandlesPrecommitValue,
    XchandlesRegistry, XchandlesSlotValue,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesRegisterAction {
    pub launcher_id: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for XchandlesRegisterAction {
    fn tree_hash(&self) -> TreeHash {
        XchandlesRegisterActionArgs::curry_tree_hash(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        )
    }
}

impl Action<XchandlesRegistry> for XchandlesRegisterAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            relative_block_height: constants.relative_block_height,
            payout_puzzle_hash: constants.precommit_payout_puzzle_hash,
        }
    }
}

impl XchandlesRegisterAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        Ok(CurriedProgram {
            program: ctx.xchandles_register_puzzle()?,
            args: XchandlesRegisterActionArgs::new(
                self.launcher_id,
                self.relative_block_height,
                self.payout_puzzle_hash,
            ),
        }
        .to_clvm(ctx)?)
    }

    pub fn spent_slot_values(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<[XchandlesSlotValue; 2], DriverError> {
        let solution = XchandlesRegisterActionSolution::<
            NodePtr,
            NodePtr,
            NodePtr,
            NodePtr,
            NodePtr,
        >::from_clvm(ctx, solution)?;

        Ok([
            XchandlesSlotValue::new(
                solution.neighbors.left_value,
                solution.left_left_value,
                solution.neighbors.right_value,
                solution.left_expiration,
                solution.left_data.owner_launcher_id,
                solution.left_data.resolved_data,
            ),
            XchandlesSlotValue::new(
                solution.neighbors.right_value,
                solution.neighbors.left_value,
                solution.right_right_value,
                solution.right_expiration,
                solution.right_data.owner_launcher_id,
                solution.right_data.resolved_data,
            ),
        ])
    }

    pub fn created_slot_values(
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<[XchandlesSlotValue; 3], DriverError> {
        let solution = XchandlesRegisterActionSolution::<
            NodePtr,
            NodePtr,
            NodePtr,
            NodePtr,
            NodePtr,
        >::from_clvm(ctx, solution)?;

        let pricing_output = ctx.run(
            solution.pricing_puzzle_reveal,
            solution.pricing_puzzle_solution,
        )?;
        let registration_time_delta = <(NodePtr, u64)>::from_clvm(ctx, pricing_output)?.1;

        let (start_time, _) = ctx.extract::<(u64, NodePtr)>(solution.pricing_puzzle_solution)?;

        Ok([
            XchandlesSlotValue::new(
                solution.neighbors.left_value,
                solution.left_left_value,
                solution.handle_hash,
                solution.left_expiration,
                solution.left_data.owner_launcher_id,
                solution.left_data.resolved_data,
            ),
            XchandlesSlotValue::new(
                solution.handle_hash,
                solution.neighbors.left_value,
                solution.neighbors.right_value,
                start_time + registration_time_delta,
                solution.data.owner_launcher_id,
                solution.data.resolved_data,
            ),
            XchandlesSlotValue::new(
                solution.neighbors.right_value,
                solution.handle_hash,
                solution.right_right_value,
                solution.right_expiration,
                solution.right_data.owner_launcher_id,
                solution.right_data.resolved_data,
            ),
        ])
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        left_slot: Slot<XchandlesSlotValue>,
        right_slot: Slot<XchandlesSlotValue>,
        precommit_coin: PrecommitCoin<XchandlesPrecommitValue>,
        base_handle_price: u64,
        registration_period: u64,
        start_time: u64,
    ) -> Result<Conditions, DriverError> {
        let handle: String = precommit_coin.value.handle.clone();
        let handle_hash: Bytes32 = handle.tree_hash().into();
        let (left_slot, right_slot) = registry.actual_neigbors(handle_hash, left_slot, right_slot);

        let secret = precommit_coin.value.secret;

        let num_periods = precommit_coin.coin.amount
            / XchandlesFactorPricingPuzzleArgs::get_price(base_handle_price, &handle, 1);

        // calculate announcement
        let mut register_announcement: Vec<u8> = precommit_coin.coin.puzzle_hash.to_vec();
        register_announcement.insert(0, b'r');

        // spend precommit coin
        let my_inner_puzzle_hash: Bytes32 = registry.info.inner_puzzle_hash().into();
        precommit_coin.spend(
            ctx,
            1, // mode 1 = register/expire (use value)
            my_inner_puzzle_hash,
        )?;

        // spend self
        let action_solution = XchandlesRegisterActionSolution {
            handle_hash,
            pricing_puzzle_reveal: XchandlesFactorPricingPuzzleArgs::get_puzzle(
                ctx,
                base_handle_price,
                registration_period,
            )?,
            pricing_puzzle_solution: XchandlesPricingSolution {
                buy_time: start_time,
                current_expiration: 0,
                handle: handle.clone(),
                num_periods,
            },
            cat_maker_reveal: DefaultCatMakerArgs::get_puzzle(
                ctx,
                precommit_coin.asset_id.tree_hash().into(),
            )?,
            cat_maker_solution: (),
            neighbors: SlotNeigborsInfo {
                left_value: left_slot.info.value.handle_hash,
                right_value: right_slot.info.value.handle_hash,
            },
            left_left_value: left_slot.info.value.neighbors.left_value,
            left_expiration: left_slot.info.value.expiration,
            left_data: left_slot.info.value.rest_data(),
            right_right_value: right_slot.info.value.neighbors.right_value,
            right_expiration: right_slot.info.value.expiration,
            right_data: right_slot.info.value.rest_data(),
            data: XchandlesDataValue {
                owner_launcher_id: precommit_coin.value.owner_launcher_id,
                resolved_data: precommit_coin.value.resolved_data,
            },
            refund_puzzle_hash_hash: precommit_coin.refund_puzzle_hash.tree_hash().into(),
            secret,
        }
        .to_clvm(ctx)?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slots
        left_slot.spend(ctx, my_inner_puzzle_hash)?;
        right_slot.spend(ctx, my_inner_puzzle_hash)?;

        Ok(
            Conditions::new().assert_puzzle_announcement(announcement_id(
                registry.coin.puzzle_hash,
                register_announcement,
            )),
        )
    }
}

pub const XCHANDLES_REGISTER_PUZZLE: [u8; 1345] = hex!("ff02ffff01ff02ffff03ffff22ffff09ff4fffff0bffff0101ff820b6f8080ffff20ff82056f80ffff0aff4fff8213ef80ffff0aff821befff4f80ffff09ff57ffff02ff2effff04ff02ffff04ff8202efff8080808080ffff09ff81b7ffff02ff2effff04ff02ffff04ff81afff8080808080ffff09ffff0dff8309ffef80ffff012080ffff15ffff0141ffff0dff830dffef808080ffff01ff04ff17ffff02ff1affff04ff02ffff04ffff02ff8202efffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ff830bffef80ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ffff02ff2effff04ff02ffff04ffff04ffff04ffff04ff57ff8205ef80ffff04ff81b7ff82016f8080ffff04ffff04ff820b6fff8317ffef80ffff04ff8309ffefff830dffef808080ff808080808080ffff0bff3cff62ff42808080ff42808080ff42808080ff8205ef8080ffff04ffff05ffff02ff81afff82016f8080ffff04ffff04ffff04ff10ffff04ff82026fff808080ffff04ffff02ff3effff04ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff8213efffff04ff8217efff821bef8080ffff04ff822fefff825fef8080ff80808080ff8080808080ffff04ffff02ff3effff04ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff821befffff04ff8213efff82bfef8080ffff04ff83017fefff8302ffef8080ff80808080ff8080808080ffff04ffff02ff16ffff04ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff4fff820bef80ffff04ffff10ff82026fffff06ffff02ff81afff82016f808080ff8305ffef8080ff80808080ff8080808080ffff04ffff02ff16ffff04ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff8213efffff04ff8217efff4f8080ffff04ff822fefff825fef8080ff80808080ff8080808080ffff04ffff02ff16ffff04ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff821befffff04ff4fff82bfef8080ffff04ff83017fefff8302ffef8080ff80808080ff8080808080ff80808080808080ff80808080808080ffff01ff088080ff0180ffff04ffff01ffffff5133ff3eff4202ffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ffff04ff2cffff04ffff0113ffff04ffff0101ffff04ff05ffff04ff0bff808080808080ffff04ffff04ff14ffff04ffff0effff0172ff0580ff808080ff178080ffff04ff18ffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ff0b8080ffff0bff3cff62ff42808080ff42808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04ff2cffff04ffff0112ffff04ff80ffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ff0b8080ffff0bff3cff62ff42808080ff42808080ff8080808080ff018080");

pub const XCHANDLES_REGISTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    07848cf0db85d13490c15331a065364add5f5b52d8059c410f1ff7aa87e66722
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesRegisterActionArgs {
    pub precommit_1st_curry_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

impl XchandlesRegisterActionArgs {
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

impl XchandlesRegisterActionArgs {
    pub fn curry_tree_hash(
        launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> TreeHash {
        CurriedProgram {
            program: XCHANDLES_REGISTER_PUZZLE_HASH,
            args: XchandlesRegisterActionArgs::new(
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
pub struct XchandlesRegisterActionSolution<PP, PS, CMP, CMS, S> {
    pub handle_hash: Bytes32,
    pub pricing_puzzle_reveal: PP,
    pub pricing_puzzle_solution: PS,
    pub cat_maker_reveal: CMP,
    pub cat_maker_solution: CMS,
    pub neighbors: SlotNeigborsInfo,
    pub left_left_value: Bytes32,
    pub left_expiration: u64,
    pub left_data: XchandlesDataValue,
    pub right_right_value: Bytes32,
    pub right_expiration: u64,
    pub right_data: XchandlesDataValue,
    pub data: XchandlesDataValue,
    pub refund_puzzle_hash_hash: Bytes32,
    pub secret: S,
}

pub const XCHANDLES_FACTOR_PRICING_PUZZLE: [u8; 475] = hex!("ff02ffff01ff02ffff03ffff15ff7fff8080ffff01ff04ffff12ff7fff05ffff02ff06ffff04ff02ffff04ffff0dff5f80ffff04ffff02ff04ffff04ff02ffff04ff5fff80808080ff808080808080ffff12ff7fff0b8080ffff01ff088080ff0180ffff04ffff01ffff02ffff03ff05ffff01ff02ffff03ffff22ffff15ffff0cff05ff80ffff010180ffff016080ffff15ffff017bffff0cff05ff80ffff0101808080ffff01ff02ff04ffff04ff02ffff04ffff0cff05ffff010180ff80808080ffff01ff02ffff03ffff22ffff15ffff0cff05ff80ffff010180ffff012f80ffff15ffff013affff0cff05ff80ffff0101808080ffff01ff10ffff0101ffff02ff04ffff04ff02ffff04ffff0cff05ffff010180ff8080808080ffff01ff088080ff018080ff0180ff8080ff0180ff05ffff14ffff02ffff03ffff15ff05ffff010280ffff01ff02ffff03ffff15ff05ffff010480ffff01ff02ffff03ffff09ff05ffff010580ffff01ff0110ffff01ff02ffff03ffff15ff05ffff011f80ffff01ff0880ffff01ff010280ff018080ff0180ffff01ff02ffff03ffff09ff05ffff010380ffff01ff01820080ffff01ff014080ff018080ff0180ffff01ff088080ff0180ffff03ff0bffff0102ffff0101808080ff018080");

pub const XCHANDLES_FACTOR_PRICING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    a7edc890e6c256e4e729e826e7b45ad0616ec8d431e4e051ee68ddf4cae868bb
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesFactorPricingPuzzleArgs {
    pub base_price: u64,
    pub registration_period: u64,
}

impl XchandlesFactorPricingPuzzleArgs {
    pub fn new(base_price: u64, registration_period: u64) -> Self {
        Self {
            base_price,
            registration_period,
        }
    }

    pub fn get_puzzle(
        ctx: &mut SpendContext,
        base_price: u64,
        registration_period: u64,
    ) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.xchandles_factor_pricing_puzzle()?,
            args: XchandlesFactorPricingPuzzleArgs::new(base_price, registration_period),
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    pub fn get_price(base_price: u64, handle: &str, num_periods: u64) -> u64 {
        base_price
            * match handle.len() {
                3 => 128,
                4 => 64,
                5 => 16,
                _ => 2,
            }
            / if handle.contains(|c: char| c.is_numeric()) {
                2
            } else {
                1
            }
            * num_periods
    }
}

impl XchandlesFactorPricingPuzzleArgs {
    pub fn curry_tree_hash(base_price: u64, registration_period: u64) -> TreeHash {
        CurriedProgram {
            program: XCHANDLES_FACTOR_PRICING_PUZZLE_HASH,
            args: XchandlesFactorPricingPuzzleArgs::new(base_price, registration_period),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesPricingSolution {
    pub buy_time: u64,
    pub current_expiration: u64,
    pub handle: String,
    #[clvm(rest)]
    pub num_periods: u64,
}

#[cfg(test)]
mod tests {
    use clvmr::reduction::EvalErr;

    use super::*;

    #[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
    #[clvm(list)]
    pub struct XchandlesFactorPricingOutput {
        pub price: u64,
        #[clvm(rest)]
        pub registered_time: u64,
    }

    #[test]
    fn test_factor_pricing_puzzle() -> Result<(), DriverError> {
        let mut ctx = SpendContext::new();
        let base_price = 1; // puzzle will only spit out factors
        let registration_period = 366 * 24 * 60 * 60; // one year

        let puzzle = XchandlesFactorPricingPuzzleArgs::get_puzzle(
            &mut ctx,
            base_price,
            registration_period,
        )?;

        for handle_length in 3..=31 {
            for num_periods in 1..=3 {
                for has_number in [false, true] {
                    let handle = if has_number {
                        "a".repeat(handle_length - 1) + "1"
                    } else {
                        "a".repeat(handle_length)
                    };

                    let solution = ctx.alloc(&XchandlesPricingSolution {
                        buy_time: 0,
                        current_expiration: (handle_length - 3) as u64, // shouldn't matter
                        handle,
                        num_periods,
                    })?;

                    let output = ctx.run(puzzle, solution)?;
                    let output = ctx.extract::<XchandlesFactorPricingOutput>(output)?;

                    let mut expected_price = if handle_length == 3 {
                        128
                    } else if handle_length == 4 {
                        64
                    } else if handle_length == 5 {
                        16
                    } else {
                        2
                    };
                    if has_number {
                        expected_price /= 2;
                    }
                    expected_price *= num_periods;

                    assert_eq!(output.price, expected_price);
                    assert_eq!(output.registered_time, num_periods * registration_period);
                }
            }
        }

        // make sure the puzzle won't let us register a handle of length 2

        let solution = ctx.alloc(&XchandlesPricingSolution {
            buy_time: 0,
            current_expiration: 0,
            handle: "aa".to_string(),
            num_periods: 1,
        })?;

        let Err(DriverError::Eval(EvalErr(_, s))) = ctx.run(puzzle, solution) else {
            panic!("Expected error");
        };
        assert_eq!(s, "clvm raise");

        // make sure the puzzle won't let us register a handle of length 32

        let solution = ctx.alloc(&XchandlesPricingSolution {
            buy_time: 0,
            current_expiration: 0,
            handle: "a".repeat(32),
            num_periods: 1,
        })?;

        let Err(DriverError::Eval(EvalErr(_, s))) = ctx.run(puzzle, solution) else {
            panic!("Expected error");
        };
        assert_eq!(s, "clvm raise");

        // make sure the puzzle won't let us register a handle with invalid characters

        let solution = ctx.alloc(&XchandlesPricingSolution {
            buy_time: 0,
            current_expiration: 0,
            handle: "yak@test".to_string(),
            num_periods: 1,
        })?;

        let Err(DriverError::Eval(EvalErr(_, s))) = ctx.run(puzzle, solution) else {
            panic!("Expected error");
        };
        assert_eq!(s, "clvm raise");

        Ok(())
    }
}
