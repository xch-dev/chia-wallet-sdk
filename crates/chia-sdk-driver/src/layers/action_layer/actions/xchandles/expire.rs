use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        DefaultCatMakerArgs, PrecommitSpendMode, XchandlesDataValue, XchandlesExpireActionArgs,
        XchandlesExpireActionSolution, XchandlesExponentialPremiumRenewPuzzleArgs,
        XchandlesFactorPricingPuzzleArgs, XchandlesPricingSolution, XchandlesSlotValue,
        PREMIUM_BITS_LIST, PREMIUM_PRECISION,
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
pub struct XchandlesExpireAction {
    pub launcher_id: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for XchandlesExpireAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        )
        .curry_tree_hash()
    }
}

impl SingletonAction<XchandlesRegistry> for XchandlesExpireAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            relative_block_height: constants.relative_block_height,
            payout_puzzle_hash: constants.precommit_payout_puzzle_hash,
        }
    }
}

impl XchandlesExpireAction {
    pub fn new_args(
        launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> XchandlesExpireActionArgs {
        XchandlesExpireActionArgs {
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

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesSlotValue, DriverError> {
        // truths for epired solution are: Buy_Time, Current_Expiration, Handle
        let solution = XchandlesExpireActionSolution::<
            NodePtr,
            NodePtr,
            NodePtr,
            (NodePtr, (u64, (String, NodePtr))),
            NodePtr,
        >::from_clvm(ctx, solution)?;

        let handle = solution.expired_handle_pricing_puzzle_solution.1 .1 .0;
        let current_expiration = solution.expired_handle_pricing_puzzle_solution.1 .0;

        Ok(XchandlesSlotValue::new(
            handle.tree_hash().into(),
            solution.neighbors.left_value,
            solution.neighbors.right_value,
            current_expiration,
            solution.old_rest.owner_launcher_id,
            solution.old_rest.resolved_data,
        ))
    }

    pub fn created_slot_value(
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesSlotValue, DriverError> {
        let solution = ctx.extract::<XchandlesExpireActionSolution<
            NodePtr,
            NodePtr,
            NodePtr,
            NodePtr,
            NodePtr,
        >>(solution)?;

        let pricing_output = ctx.run(
            solution.expired_handle_pricing_puzzle_reveal,
            solution.expired_handle_pricing_puzzle_solution,
        )?;
        let registration_time_delta = <(NodePtr, u64)>::from_clvm(ctx, pricing_output)?.1;

        // truths are: Buy_Time, Current_Expiration, Handle
        let (buy_time, (_, (handle, _))) = ctx.extract::<(u64, (NodePtr, (String, NodePtr)))>(
            solution.expired_handle_pricing_puzzle_solution,
        )?;

        Ok(XchandlesSlotValue::new(
            handle.tree_hash().into(),
            solution.neighbors.left_value,
            solution.neighbors.right_value,
            buy_time + registration_time_delta,
            solution.new_rest.owner_launcher_id,
            solution.new_rest.resolved_data,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        slot: Slot<XchandlesSlotValue>,
        num_periods: u64,
        base_handle_price: u64,
        registration_period: u64,
        precommit_coin: PrecommitCoin<XchandlesPrecommitValue>,
        start_time: u64,
    ) -> Result<Conditions, DriverError> {
        let my_inner_puzzle_hash = registry.info.inner_puzzle_hash().into();

        // announcement is simply premcommitment coin ph
        let expire_ann = precommit_coin.coin.puzzle_hash;

        // spend precommit coin
        precommit_coin.spend(ctx, PrecommitSpendMode::REGISTER, my_inner_puzzle_hash)?;

        // spend self
        let slot = registry.actual_slot(slot);
        let expire_args =
            XchandlesExpirePricingPuzzle::from_info(ctx, base_handle_price, registration_period)?;
        let action_solution = XchandlesExpireActionSolution {
            cat_maker_puzzle_reveal: ctx.curry(DefaultCatMakerArgs::new(
                precommit_coin.asset_id.tree_hash().into(),
            ))?,
            cat_maker_puzzle_solution: (),
            expired_handle_pricing_puzzle_reveal: ctx.curry(expire_args)?,
            expired_handle_pricing_puzzle_solution: XchandlesPricingSolution {
                buy_time: start_time,
                current_expiration: slot.info.value.expiration,
                handle: precommit_coin.value.handle.clone(),
                num_periods,
            },
            refund_puzzle_hash_hash: precommit_coin.refund_puzzle_hash.tree_hash().into(),
            secret: precommit_coin.value.secret,
            neighbors: slot.info.value.neighbors,
            old_rest: slot.info.value.rest_data(),
            new_rest: XchandlesDataValue {
                owner_launcher_id: precommit_coin.value.owner_launcher_id,
                resolved_data: precommit_coin.value.resolved_data,
            },
        }
        .to_clvm(ctx)?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slot
        slot.spend(ctx, my_inner_puzzle_hash)?;

        let mut expire_ann = expire_ann.to_vec();
        expire_ann.insert(0, b'x');
        Ok(Conditions::new()
            .assert_puzzle_announcement(announcement_id(registry.coin.puzzle_hash, expire_ann)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesExpirePricingPuzzle {}

impl XchandlesExpirePricingPuzzle {
    // A scale factor is how many units of the payment token equate to $1
    // For exampe, you'd use scale_factor=1000 for wUSDC.b
    pub fn from_info(
        ctx: &mut SpendContext,
        base_price: u64,
        registration_period: u64,
    ) -> Result<XchandlesExponentialPremiumRenewPuzzleArgs<NodePtr>, DriverError> {
        Ok(XchandlesExponentialPremiumRenewPuzzleArgs {
            base_program: ctx.curry(XchandlesFactorPricingPuzzleArgs {
                base_price,
                registration_period,
            })?,
            halving_period: 86400, // one day = 86400 = 60 * 60 * 24 seconds
            start_premium: XchandlesExponentialPremiumRenewPuzzleArgs::<()>::get_start_premium(
                1000,
            ),
            end_value: XchandlesExponentialPremiumRenewPuzzleArgs::<()>::get_end_value(1000),
            precision: PREMIUM_PRECISION,
            bits_list: PREMIUM_BITS_LIST.to_vec(),
        })
    }

    pub fn curry_tree_hash(base_price: u64, registration_period: u64) -> TreeHash {
        XchandlesExponentialPremiumRenewPuzzleArgs::<TreeHash> {
            base_program: XchandlesFactorPricingPuzzleArgs {
                base_price,
                registration_period,
            }
            .curry_tree_hash(),
            halving_period: 86400, // one day = 86400 = 60 * 60 * 24 seconds
            start_premium: XchandlesExponentialPremiumRenewPuzzleArgs::<()>::get_start_premium(
                1000,
            ),
            end_value: XchandlesExponentialPremiumRenewPuzzleArgs::<()>::get_end_value(1000),
            precision: PREMIUM_PRECISION,
            bits_list: PREMIUM_BITS_LIST.to_vec(),
        }
        .curry_tree_hash()
    }

    pub fn get_price(
        ctx: &mut SpendContext,
        args: XchandlesExponentialPremiumRenewPuzzleArgs<NodePtr>,
        handle: String,
        expiration: u64,
        buy_time: u64,
        num_periods: u64,
    ) -> Result<u128, DriverError> {
        let puzzle = ctx.curry(args)?;
        let solution = ctx.alloc(&XchandlesPricingSolution {
            buy_time,
            current_expiration: expiration,
            handle,
            num_periods,
        })?;
        let output = ctx.run(puzzle, solution)?;

        Ok(ctx.extract::<(u128, u64)>(output)?.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(FromClvm, ToClvm, Debug, Copy, Clone, PartialEq, Eq)]
    #[clvm(list)]
    struct XchandlesPricingOutput {
        pub price: u128,
        #[clvm(rest)]
        pub registered_time: u64,
    }

    #[test]
    fn test_exponential_premium_puzzle() -> Result<(), DriverError> {
        let mut ctx = SpendContext::new();

        let registration_period = 366 * 24 * 60 * 60;
        let exponential_args =
            XchandlesExpirePricingPuzzle::from_info(&mut ctx, 0, registration_period)?;
        let puzzle = ctx.curry(exponential_args.clone())?;

        let mut last_price = 100_000_000_000;
        for day in 0..28 {
            for hour in 0..24 {
                let buy_time = day * 24 * 60 * 60 + hour * 60 * 60;
                let solution = ctx.alloc(&XchandlesPricingSolution {
                    buy_time,
                    current_expiration: 0,
                    handle: "yakuhito".to_string(),
                    num_periods: 1,
                })?;

                let output = ctx.run(puzzle, solution)?;
                let output = ctx.extract::<XchandlesPricingOutput>(output)?;

                assert_eq!(output.registered_time, 366 * 24 * 60 * 60);

                if hour == 0 {
                    let scale_factor =
                        372_529_029_846_191_406_u128 * 1000_u128 / 1_000_000_000_000_000_000_u128;
                    assert_eq!(
                        output.price,
                        (100_000_000 * 1000) / (1 << day) - scale_factor
                    );
                }

                assert!(output.price < last_price);
                last_price = output.price;

                assert_eq!(
                    XchandlesExpirePricingPuzzle::get_price(
                        &mut ctx,
                        exponential_args.clone(),
                        "yakuhito".to_string(),
                        0,
                        buy_time,
                        1
                    )?,
                    output.price
                );
            }
        }

        // check premium after auction is 0
        let solution = ctx.alloc(&XchandlesPricingSolution {
            buy_time: 28 * 24 * 60 * 60,
            current_expiration: 0,
            handle: "yakuhito".to_string(),
            num_periods: 1,
        })?;

        let output = ctx.run(puzzle, solution)?;
        let output = ctx.extract::<XchandlesPricingOutput>(output)?;

        assert_eq!(output.registered_time, 366 * 24 * 60 * 60);
        assert_eq!(output.price, 0);

        Ok(())
    }
}
