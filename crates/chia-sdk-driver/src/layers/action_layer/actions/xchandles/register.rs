use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::{SingletonArgs, SingletonStruct};
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_sdk_types::{
    Conditions, Mod, announcement_id,
    puzzles::{
        DefaultCatMakerArgs, PrecommitSpendMode, PuzzleAndSolution, SlotNeigborsInfo,
        XchandlesFactorPricingPuzzleArgs, XchandlesHandleSlotValue, XchandlesNewDataPuzzleHashes,
        XchandlesOtherPrecommitData, XchandlesPricingSolution, XchandlesRegisterActionArgs,
        XchandlesRegisterActionSolution, XchandlesRestOfSlot, XchandlesSlotNonce,
    },
};
use clvm_traits::ToClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    Asset, DriverError, PrecommitCoin, PrecommitLayer, SingletonAction, Slot, Spend, SpendContext,
    XchandlesConstants, XchandlesPrecommitValue, XchandlesRegistry,
    XchandlesRegistryCreatedAnnouncementPrefix, XchandlesRegistryReceivedMessagePrefix,
};

use super::{
    XchandlesExpirePricingPuzzle, XchandlesPrecommitValueLog, XchandlesRegisterActionLog,
    run_pricing_output,
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
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
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
    ) -> Result<XchandlesRegisterActionLog, DriverError> {
        let solution = ctx.extract::<XchandlesRegisterActionSolution<
            NodePtr,
            NodePtr,
            NodePtr,
            NodePtr,
            Bytes32,
        >>(solution)?;

        let spent_left_slot = XchandlesHandleSlotValue::new(
            solution.left_rest_of_slot.this_counter,
            solution.neighbors.left_value,
            solution.left_rest_of_slot.this_this_value,
            solution.neighbors.right_value,
            solution.left_rest_of_slot.this_expiration,
            solution.left_rest_of_slot.this_data.owner_launcher_id,
            solution.left_rest_of_slot.this_data.resolved_launcher_id,
        );
        let spent_right_slot = XchandlesHandleSlotValue::new(
            solution.right_rest_of_slot.this_counter,
            solution.neighbors.right_value,
            solution.neighbors.left_value,
            solution.right_rest_of_slot.this_this_value,
            solution.right_rest_of_slot.this_expiration,
            solution.right_rest_of_slot.this_data.owner_launcher_id,
            solution.right_rest_of_slot.this_data.resolved_launcher_id,
        );

        let pricing_solution =
            ctx.extract::<XchandlesPricingSolution>(solution.pricing_puzzle_and_solution.solution)?;

        let (total_price, registered_time) = run_pricing_output(
            ctx,
            solution.pricing_puzzle_and_solution.puzzle,
            solution.pricing_puzzle_and_solution.solution,
        )?;

        let created_left_slot = XchandlesHandleSlotValue::new(
            solution.left_rest_of_slot.this_counter + 1,
            solution.neighbors.left_value,
            solution.left_rest_of_slot.this_this_value,
            solution.handle_hash,
            solution.left_rest_of_slot.this_expiration,
            solution.left_rest_of_slot.this_data.owner_launcher_id,
            solution.left_rest_of_slot.this_data.resolved_launcher_id,
        );
        let created_handle_slot = XchandlesHandleSlotValue::new(
            0,
            solution.handle_hash,
            solution.neighbors.left_value,
            solution.neighbors.right_value,
            pricing_solution.buy_time + registered_time,
            solution.other_precommit_data.launcher_ids.owner_launcher_id,
            solution
                .other_precommit_data
                .launcher_ids
                .resolved_launcher_id,
        );
        let created_right_slot = XchandlesHandleSlotValue::new(
            solution.right_rest_of_slot.this_counter + 1,
            solution.neighbors.right_value,
            solution.handle_hash,
            solution.right_rest_of_slot.this_this_value,
            solution.right_rest_of_slot.this_expiration,
            solution.right_rest_of_slot.this_data.owner_launcher_id,
            solution.right_rest_of_slot.this_data.resolved_launcher_id,
        );

        let handle = pricing_solution.handle.clone();
        let cat_maker_puzzle_hash: Bytes32 = ctx
            .tree_hash(solution.cat_maker_puzzle_and_solution.puzzle)
            .into();
        let pricing_puzzle_hash: Bytes32 = ctx
            .tree_hash(solution.pricing_puzzle_and_solution.puzzle)
            .into();
        let precommit_value = XchandlesPrecommitValueLog::new(
            cat_maker_puzzle_hash,
            (),
            pricing_puzzle_hash,
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
                .data_puzzle_hashes
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
                            .data_puzzle_hashes
                            .new_resolved_inner_puzzle_hash
                            .into(),
                    )
                    .into(),
                )
            };

        Ok(XchandlesRegisterActionLog {
            spent_left_slot,
            spent_right_slot,
            created_left_slot,
            created_handle_slot,
            created_right_slot,
            precommit_value,
            total_price,
            registered_time,
            owner_full_puzzle_hash,
            resolved_full_puzzle_hash,
            owner_inner_puzzle_hash: solution.data_puzzle_hashes.new_owner_inner_puzzle_hash,
            resolved_inner_puzzle_hash: solution.data_puzzle_hashes.new_resolved_inner_puzzle_hash,
        })
    }

    /// Historical factor-pricing register path. Prefer
    /// [`Self::spend_with_pricing`] when the precommit committed the deployed
    /// expiry-pricing puzzle.
    #[allow(clippy::too_many_arguments)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        left_slot: Slot<XchandlesHandleSlotValue>,
        right_slot: Slot<XchandlesHandleSlotValue>,
        precommit_coin: &PrecommitCoin<XchandlesPrecommitValue>,
        base_handle_price: u64,
        registration_period: u64,
        start_time: u64,
        owner_inner_puzzle_hash: Bytes32,
        resolved_inner_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, Conditions, Option<Conditions>), DriverError> {
        let handle = precommit_coin.value.handle.clone();
        let num_periods = precommit_coin.coin.amount()
            / XchandlesFactorPricingPuzzleArgs::get_price(base_handle_price, &handle, 1);
        let pricing_puzzle = ctx.curry(XchandlesFactorPricingPuzzleArgs {
            base_price: base_handle_price,
            registration_period,
        })?;
        let pricing_solution = XchandlesPricingSolution {
            buy_time: start_time,
            current_expiration: 0,
            handle,
            num_periods,
        };
        self.spend_with_pricing(
            ctx,
            registry,
            left_slot,
            right_slot,
            precommit_coin,
            pricing_puzzle,
            pricing_solution,
            owner_inner_puzzle_hash,
            resolved_inner_puzzle_hash,
        )
    }

    /// Register using an explicit pricing puzzle reveal and solution.
    ///
    /// Used for the deployed expiry-pricing precommit (ordinary
    /// `current_expiration = 0`) so the reveal matches the commitment instead
    /// of silently substituting factor pricing.
    #[allow(clippy::too_many_arguments)]
    pub fn spend_with_pricing(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        left_slot: Slot<XchandlesHandleSlotValue>,
        right_slot: Slot<XchandlesHandleSlotValue>,
        precommit_coin: &PrecommitCoin<XchandlesPrecommitValue>,
        pricing_puzzle: NodePtr,
        pricing_solution: XchandlesPricingSolution,
        owner_inner_puzzle_hash: Bytes32,
        resolved_inner_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, Conditions, Option<Conditions>), DriverError> {
        let handle_hash = pricing_solution.handle.tree_hash().into();
        let (left_slot, right_slot) = registry.actual_neigbors(handle_hash, left_slot, right_slot);

        let secret = precommit_coin.value.secret;

        // calculate announcement
        let register_announcement =
            XchandlesRegistryCreatedAnnouncementPrefix::register(precommit_coin.coin.puzzle_hash);
        let new_owner_message =
            XchandlesRegistryReceivedMessagePrefix::register_owner(precommit_coin.coin.puzzle_hash);
        let new_resolved_message = if precommit_coin.value.resolved_launcher_id
            == precommit_coin.value.owner_launcher_id
        {
            None
        } else {
            Some(XchandlesRegistryReceivedMessagePrefix::register_resolved(
                precommit_coin.coin.puzzle_hash,
            ))
        };

        // spend precommit coin
        let my_inner_puzzle_hash = registry.info.inner_puzzle_hash().into();
        precommit_coin.spend(ctx, PrecommitSpendMode::REGISTER, my_inner_puzzle_hash)?;

        // spend self
        let action_solution = XchandlesRegisterActionSolution {
            handle_hash,
            neighbors: SlotNeigborsInfo {
                left_value: left_slot.info.value.handle_hash,
                right_value: right_slot.info.value.handle_hash,
            },
            cat_maker_puzzle_and_solution: PuzzleAndSolution::new(
                ctx.curry(DefaultCatMakerArgs::new(
                    precommit_coin.asset_id.tree_hash().into(),
                ))?,
                (),
            ),
            pricing_puzzle_and_solution: PuzzleAndSolution::new(pricing_puzzle, pricing_solution),
            left_rest_of_slot: XchandlesRestOfSlot::new(
                left_slot.info.value.counter,
                left_slot.info.value.neighbors.left_value,
                left_slot.info.value.expiration,
                left_slot.info.value.rest_data(),
            ),
            right_rest_of_slot: XchandlesRestOfSlot::new(
                right_slot.info.value.counter,
                right_slot.info.value.neighbors.right_value,
                right_slot.info.value.expiration,
                right_slot.info.value.rest_data(),
            ),
            data_puzzle_hashes: XchandlesNewDataPuzzleHashes::new(
                owner_inner_puzzle_hash,
                resolved_inner_puzzle_hash,
            ),
            other_precommit_data: XchandlesOtherPrecommitData::new(
                precommit_coin.value.owner_launcher_id,
                precommit_coin.value.resolved_launcher_id,
                precommit_coin.refund_puzzle_hash.tree_hash().into(),
                secret,
            ),
        }
        .to_clvm(ctx)?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slots
        left_slot.spend(ctx, my_inner_puzzle_hash)?;
        right_slot.spend(ctx, my_inner_puzzle_hash)?;

        let message_destination = ctx.alloc(&registry.coin.puzzle_hash)?;
        Ok((
            Conditions::new().assert_puzzle_announcement(announcement_id(
                registry.coin.puzzle_hash,
                register_announcement,
            )),
            Conditions::new().send_message(18, new_owner_message.into(), vec![message_destination]),
            new_resolved_message.map(|message| {
                Conditions::new().send_message(18, message.into(), vec![message_destination])
            }),
        ))
    }

    /// Curry the deployed expiry-pricing puzzle for an ordinary-register reveal.
    pub fn expiry_pricing_puzzle(
        ctx: &mut SpendContext,
        base_handle_price: u64,
        registration_period: u64,
    ) -> Result<NodePtr, DriverError> {
        let args =
            XchandlesExpirePricingPuzzle::from_info(ctx, base_handle_price, registration_period)?;
        ctx.curry(args)
    }
}

#[cfg(test)]
mod tests {
    use clvm_traits::FromClvm;
    use clvmr::error::EvalErr;

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
        const LONG_PREMINE_HANDLES: &[&str] = &[
            "ashorttermmindgetsinthewayofalongtermgrind",
            "bigbouncingthicctwerkingthunderclappingbadonkabooty",
            "bigfathonkingjigglymommymilkerboobies",
            "rolexislandpermutoplatinumlamboempirexrp404inu",
            "thankstopawketforprovidingthebestchiasdk",
        ];

        let mut ctx = SpendContext::new();
        let base_price = 1; // puzzle will only spit out factors
        let registration_period = 366 * 24 * 60 * 60; // one year

        let puzzle = ctx.curry(XchandlesFactorPricingPuzzleArgs {
            base_price,
            registration_period,
        })?;

        for handle_length in 3..=63 {
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
                        handle: handle.clone(),
                        num_periods,
                    })?;

                    let output = ctx.run(puzzle, solution)?;
                    let output = ctx.extract::<XchandlesFactorPricingOutput>(output)?;

                    let expected_price = XchandlesFactorPricingPuzzleArgs::get_price(
                        base_price,
                        &handle,
                        num_periods,
                    );

                    assert_eq!(output.price, expected_price);
                    assert_eq!(output.registered_time, num_periods * registration_period);
                }
            }
        }

        // Reject lengths 0, 1, 2, and 64+.
        for handle in ["", "a", "aa", &*"a".repeat(64)] {
            let solution = ctx.alloc(&XchandlesPricingSolution {
                buy_time: 0,
                current_expiration: 0,
                handle: handle.to_string(),
                num_periods: 1,
            })?;

            let Err(DriverError::Eval(EvalErr::Raise(_))) = ctx.run(puzzle, solution) else {
                panic!("Expected clvm raise for handle {handle:?}");
            };
        }

        // Reject invalid characters (uppercase, punctuation, whitespace, non-ASCII).
        for handle in ["ABC", "yak@test", "foo bar", "café", "a.b", "a-b"] {
            let solution = ctx.alloc(&XchandlesPricingSolution {
                buy_time: 0,
                current_expiration: 0,
                handle: handle.to_string(),
                num_periods: 1,
            })?;

            let Err(DriverError::Eval(EvalErr::Raise(_))) = ctx.run(puzzle, solution) else {
                panic!("Expected clvm raise for handle {handle:?}");
            };
        }

        // Published Premine handles longer than 31 must price through the executable path.
        for handle in LONG_PREMINE_HANDLES {
            assert!(
                XchandlesFactorPricingPuzzleArgs::is_valid_handle(handle),
                "premine handle failed launch validation: {handle}"
            );
            let solution = ctx.alloc(&XchandlesPricingSolution {
                buy_time: 0,
                current_expiration: 0,
                handle: (*handle).to_string(),
                num_periods: 1,
            })?;
            let output = ctx.run(puzzle, solution)?;
            let output = ctx.extract::<XchandlesFactorPricingOutput>(output)?;
            assert_eq!(
                output.price,
                XchandlesFactorPricingPuzzleArgs::get_price(base_price, handle, 1)
            );
            assert_eq!(output.registered_time, registration_period);
        }

        Ok(())
    }

    #[test]
    fn expiry_pricing_register_reveal_matches_deployed_expire_hash() -> Result<(), DriverError> {
        let mut ctx = SpendContext::new();
        let base_price = 5_000;
        let registration_period = 31_557_600;
        let puzzle = XchandlesRegisterAction::expiry_pricing_puzzle(
            &mut ctx,
            base_price,
            registration_period,
        )?;
        assert_eq!(
            ctx.tree_hash(puzzle),
            XchandlesExpirePricingPuzzle::curry_tree_hash(base_price, registration_period)
        );
        let factor = XchandlesFactorPricingPuzzleArgs {
            base_price,
            registration_period,
        }
        .curry_tree_hash();
        assert_ne!(ctx.tree_hash(puzzle), factor);
        Ok(())
    }
}
