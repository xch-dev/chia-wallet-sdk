use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        DefaultCatMakerArgs, PrecommitSpendMode, SlotNeigborsInfo, XchandlesDataValue,
        XchandlesFactorPricingPuzzleArgs, XchandlesPricingSolution, XchandlesRegisterActionArgs,
        XchandlesRegisterActionSolution, XchandlesSlotValue,
    },
    Conditions, Mod,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, PrecommitCoin, PrecommitLayer, SingletonAction, Slot, Spend, SpendContext,
    XchandlesConstants, XchandlesPrecommitValue, XchandlesRegistry,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesRegisterAction {
    pub launcher_id: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for XchandlesRegisterAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        )
        .curry_tree_hash()
    }
}

impl SingletonAction<XchandlesRegistry> for XchandlesRegisterAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            relative_block_height: constants.relative_block_height,
            payout_puzzle_hash: constants.precommit_payout_puzzle_hash,
        }
    }
}

impl XchandlesRegisterAction {
    pub fn new_args(
        launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> XchandlesRegisterActionArgs {
        XchandlesRegisterActionArgs {
            precommit_1st_curry_hash: PrecommitLayer::<()>::first_curry_hash(
                SingletonStruct::new(launcher_id).tree_hash().into(),
                relative_block_height,
                payout_puzzle_hash,
            )
            .into(),
            slot_1st_curry_hash: Slot::<()>::first_curry_hash(launcher_id, 0).into(),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        ))
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
        let handle = precommit_coin.value.handle.clone();
        let handle_hash = handle.tree_hash().into();
        let (left_slot, right_slot) = registry.actual_neigbors(handle_hash, left_slot, right_slot);

        let secret = precommit_coin.value.secret;

        let num_periods = precommit_coin.coin.amount
            / XchandlesFactorPricingPuzzleArgs::get_price(base_handle_price, &handle, 1);

        // calculate announcement
        let mut register_announcement = precommit_coin.coin.puzzle_hash.to_vec();
        register_announcement.insert(0, b'r');

        // spend precommit coin
        let my_inner_puzzle_hash = registry.info.inner_puzzle_hash().into();
        precommit_coin.spend(ctx, PrecommitSpendMode::REGISTER, my_inner_puzzle_hash)?;

        // spend self
        let action_solution = XchandlesRegisterActionSolution {
            handle_hash,
            pricing_puzzle_reveal: ctx.curry(XchandlesFactorPricingPuzzleArgs {
                base_price: base_handle_price,
                registration_period,
            })?,
            pricing_puzzle_solution: XchandlesPricingSolution {
                buy_time: start_time,
                current_expiration: 0,
                handle: handle.clone(),
                num_periods,
            },
            cat_maker_reveal: ctx.curry(DefaultCatMakerArgs::new(
                precommit_coin.asset_id.tree_hash().into(),
            ))?,
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

#[cfg(test)]
mod tests {
    use clvmr::reduction::EvalErr;

    use super::*;

    #[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
    #[clvm(list)]
    struct XchandlesFactorPricingOutput {
        pub price: u64,
        #[clvm(rest)]
        pub registered_time: u64,
    }

    #[test]
    fn test_factor_pricing_puzzle() -> Result<(), DriverError> {
        let mut ctx = SpendContext::new();
        let base_price = 1; // puzzle will only spit out factors
        let registration_period = 366 * 24 * 60 * 60; // one year

        let puzzle = ctx.curry(XchandlesFactorPricingPuzzleArgs {
            base_price,
            registration_period,
        })?;

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
