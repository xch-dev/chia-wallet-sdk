use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::{SingletonArgs, SingletonStruct};
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_sdk_types::{
    Conditions, Mod, announcement_id,
    puzzles::{
        DefaultCatMakerArgs, PREMIUM_BITS_LIST, PREMIUM_PRECISION, PrecommitSpendMode,
        PuzzleAndSolution, XchandlesExpireActionArgs, XchandlesExpireActionSolution,
        XchandlesExponentialPremiumRenewPuzzleArgs, XchandlesFactorPricingPuzzleArgs,
        XchandlesHandleSlotValue, XchandlesNewDataPuzzleHashes, XchandlesOtherPrecommitData,
        XchandlesPricingSolution, XchandlesSlotNonce,
    },
};
use clvm_traits::ToClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, PrecommitCoin, PrecommitLayer, SingletonAction, Slot, Spend, SpendContext,
    XchandlesConstants, XchandlesPrecommitValue, XchandlesRegistry,
    XchandlesRegistryCreatedAnnouncementPrefix, XchandlesRegistryReceivedMessagePrefix,
    XchandlesRegistryState,
};

use super::{XchandlesExpireActionLog, XchandlesPrecommitValueLog, run_pricing_output};

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
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_launcher_mod_hash: SINGLETON_LAUNCHER_HASH.into(),
            precommit_1st_curry_hash: PrecommitLayer::<()>::first_curry_hash(
                SingletonStruct::new(launcher_id).tree_hash().into(),
                relative_block_height,
                payout_puzzle_hash,
            )
            .into(),
            handle_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                XchandlesSlotNonce::HANDLE.to_u64(),
            )
            .into(),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(Self::new_args(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        ))
    }

    pub fn get_log(
        ctx: &mut SpendContext,
        solution: NodePtr,
        state: XchandlesRegistryState,
    ) -> Result<XchandlesExpireActionLog, DriverError> {
        let solution = ctx.extract::<XchandlesExpireActionSolution<
            NodePtr,
            NodePtr,
            NodePtr,
            NodePtr,
            Bytes32,
        >>(solution)?;

        let pricing_solution = ctx.extract::<XchandlesPricingSolution>(
            solution.expired_handle_pricing_puzzle_and_solution.solution,
        )?;

        let spent_slot = XchandlesHandleSlotValue::new(
            solution.counter,
            pricing_solution.handle.tree_hash().into(),
            solution.neighbors.left_value,
            solution.neighbors.right_value,
            pricing_solution.current_expiration,
            solution.old_rest.owner_launcher_id,
            solution.old_rest.resolved_launcher_id,
        );

        let (total_price, registered_time) = run_pricing_output(
            ctx,
            solution.expired_handle_pricing_puzzle_and_solution.puzzle,
            solution.expired_handle_pricing_puzzle_and_solution.solution,
        )?;

        let created_slot = XchandlesHandleSlotValue::new(
            solution.counter + 1,
            spent_slot.handle_hash,
            solution.neighbors.left_value,
            solution.neighbors.right_value,
            pricing_solution.buy_time + registered_time,
            solution.other_precommit_data.launcher_ids.owner_launcher_id,
            solution
                .other_precommit_data
                .launcher_ids
                .resolved_launcher_id,
        );

        let handle = pricing_solution.handle.clone();
        let precommit_value = XchandlesPrecommitValueLog::new(
            state.cat_maker_puzzle_hash,
            (),
            state.expired_handle_pricing_puzzle_hash,
            pricing_solution,
            handle,
            solution.other_precommit_data.refund_and_secret.secret,
            solution.other_precommit_data.launcher_ids.owner_launcher_id,
            solution
                .other_precommit_data
                .launcher_ids
                .resolved_launcher_id,
        );

        let owner_full_puzzle_hash = SingletonArgs::curry_tree_hash(
            solution.other_precommit_data.launcher_ids.owner_launcher_id,
            solution
                .new_inner_puzzle_hashes
                .new_owner_inner_puzzle_hash
                .into(),
        )
        .into();

        let resolved_full_puzzle_hash =
            if solution.other_precommit_data.launcher_ids.owner_launcher_id
                == solution
                    .other_precommit_data
                    .launcher_ids
                    .resolved_launcher_id
            {
                None
            } else {
                Some(
                    SingletonArgs::curry_tree_hash(
                        solution
                            .other_precommit_data
                            .launcher_ids
                            .resolved_launcher_id,
                        solution
                            .new_inner_puzzle_hashes
                            .new_resolved_inner_puzzle_hash
                            .into(),
                    )
                    .into(),
                )
            };

        Ok(XchandlesExpireActionLog {
            spent_slot,
            created_slot,
            precommit_value,
            total_price,
            registered_time,
            owner_full_puzzle_hash,
            resolved_full_puzzle_hash,
            owner_inner_puzzle_hash: solution.new_inner_puzzle_hashes.new_owner_inner_puzzle_hash,
            resolved_inner_puzzle_hash: solution
                .new_inner_puzzle_hashes
                .new_resolved_inner_puzzle_hash,
        })
    }

    // returns:
    //  - expire general announcement
    //  - send message to be sent by the new owner
    //  - send message to be sent by the new resolved launcher (if different from the owner)
    #[allow(clippy::too_many_arguments)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        slot: Slot<XchandlesHandleSlotValue>,
        num_periods: u64,
        base_handle_price: u64,
        registration_period: u64,
        precommit_coin: &PrecommitCoin<XchandlesPrecommitValue>,
        start_time: u64,
        new_owner_inner_puzzle_hash: Bytes32,
        new_resolved_inner_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, Conditions, Option<Conditions>), DriverError> {
        let my_inner_puzzle_hash = registry.info.inner_puzzle_hash().into();

        // announcements and messages
        let precommit_coin_puzzle_hash = precommit_coin.coin.puzzle_hash;

        // spend precommit coin
        precommit_coin.spend(ctx, PrecommitSpendMode::REGISTER, my_inner_puzzle_hash)?;

        // spend self
        let slot = registry.actual_handle_slot(slot);
        let expire_args =
            XchandlesExpirePricingPuzzle::from_info(ctx, base_handle_price, registration_period)?;
        let pricing_solution = XchandlesPricingSolution {
            buy_time: start_time,
            current_expiration: slot.info.value.expiration,
            handle: precommit_coin.value.handle.clone(),
            num_periods,
        };
        let action_solution = XchandlesExpireActionSolution {
            counter: slot.info.value.counter,
            expired_handle_pricing_puzzle_and_solution: PuzzleAndSolution::new(
                ctx.curry(expire_args)?,
                pricing_solution,
            ),
            cat_maker_and_solution: PuzzleAndSolution::new(
                ctx.curry(DefaultCatMakerArgs::new(
                    precommit_coin.asset_id.tree_hash().into(),
                ))?,
                (),
            ),
            other_precommit_data: XchandlesOtherPrecommitData::new(
                precommit_coin.value.owner_launcher_id,
                precommit_coin.value.resolved_launcher_id,
                precommit_coin.refund_puzzle_hash.tree_hash().into(),
                precommit_coin.value.secret,
            ),
            neighbors: slot.info.value.neighbors,
            old_rest: slot.info.value.rest_data(),
            new_inner_puzzle_hashes: XchandlesNewDataPuzzleHashes::new(
                new_owner_inner_puzzle_hash,
                new_resolved_inner_puzzle_hash,
            ),
        }
        .to_clvm(ctx)?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slot
        slot.spend(ctx, my_inner_puzzle_hash)?;

        let message_destination = ctx.alloc(&registry.coin.puzzle_hash)?;
        Ok((
            Conditions::new().assert_puzzle_announcement(announcement_id(
                registry.coin.puzzle_hash,
                XchandlesRegistryCreatedAnnouncementPrefix::expire(precommit_coin_puzzle_hash),
            )),
            Conditions::new().send_message(
                18,
                XchandlesRegistryReceivedMessagePrefix::expire_owner(precommit_coin_puzzle_hash)
                    .into(),
                vec![message_destination],
            ),
            if precommit_coin.value.resolved_launcher_id == precommit_coin.value.owner_launcher_id {
                None
            } else {
                Some(
                    Conditions::new().send_message(
                        18,
                        XchandlesRegistryReceivedMessagePrefix::expire_resolved(
                            precommit_coin_puzzle_hash,
                        )
                        .into(),
                        vec![message_destination],
                    ),
                )
            },
        ))
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
    use clvm_traits::{FromClvm, ToClvm};

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
