use chia_protocol::Bytes32;
use chia_sdk_types::{
    Conditions, Mod, announcement_id,
    puzzles::{
        RewardDistributorEntrySlotValue, RewardDistributorInitiatePayoutActionSolution,
        RewardDistributorInitiatePayoutWithApprovalActionArgs,
        RewardDistributorInitiatePayoutWithoutApprovalActionArgs, RewardDistributorSlotNonce,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, RewardDistributor, RewardDistributorConstants,
    RewardDistributorCreatedAnnouncementPrefix, RewardDistributorInitiatePayoutActionLog,
    RewardDistributorReceivedMessagePrefix, RewardDistributorStateTransition, SingletonAction,
    Slot, Spend, SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorInitiatePayoutAction {
    pub launcher_id: Bytes32,
    pub payout_threshold: u64,
    pub precision: u64,
    pub require_approval: bool,
}

impl ToTreeHash for RewardDistributorInitiatePayoutAction {
    fn tree_hash(&self) -> TreeHash {
        if self.require_approval {
            RewardDistributorInitiatePayoutWithApprovalActionArgs {
                entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                    self.launcher_id,
                    RewardDistributorSlotNonce::ENTRY.to_u64(),
                )
                .into(),
                payout_threshold: self.payout_threshold,
                precision: self.precision,
            }
            .curry_tree_hash()
        } else {
            RewardDistributorInitiatePayoutWithoutApprovalActionArgs {
                entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                    self.launcher_id,
                    RewardDistributorSlotNonce::ENTRY.to_u64(),
                )
                .into(),
                payout_threshold: self.payout_threshold,
                precision: self.precision,
            }
            .curry_tree_hash()
        }
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorInitiatePayoutAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            payout_threshold: constants.payout_threshold,
            precision: constants.precision,
            require_approval: constants.require_payout_approval,
        }
    }
}

impl RewardDistributorInitiatePayoutAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        if self.require_approval {
            ctx.curry(RewardDistributorInitiatePayoutWithApprovalActionArgs {
                entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                    self.launcher_id,
                    RewardDistributorSlotNonce::ENTRY.to_u64(),
                )
                .into(),
                payout_threshold: self.payout_threshold,
                precision: self.precision,
            })
        } else {
            ctx.curry(RewardDistributorInitiatePayoutWithoutApprovalActionArgs {
                entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                    self.launcher_id,
                    RewardDistributorSlotNonce::ENTRY.to_u64(),
                )
                .into(),
                payout_threshold: self.payout_threshold,
                precision: self.precision,
            })
        }
    }

    pub fn get_log(
        ctx: &SpendContext,
        solution: NodePtr,
        changes: RewardDistributorStateTransition,
    ) -> Result<RewardDistributorInitiatePayoutActionLog, DriverError> {
        let params = ctx.extract::<RewardDistributorInitiatePayoutActionSolution>(solution)?;

        Ok(RewardDistributorInitiatePayoutActionLog {
            spent_entry_slot: RewardDistributorEntrySlotValue {
                counter: params.slot_counter,
                payout_puzzle_hash: params.entry_payout_puzzle_hash,
                initial_cumulative_payout: params.entry_initial_cumulative_payout,
                shares: params.entry_shares,
            },
            created_entry_slot: RewardDistributorEntrySlotValue {
                counter: params.slot_counter + 1,
                payout_puzzle_hash: params.entry_payout_puzzle_hash,
                initial_cumulative_payout: changes.old_state.round_reward_info.cumulative_payout,
                shares: params.entry_shares,
            },
            entry_payout_amount: params.entry_payout_amount,
            payout_rounding_error: params.payout_rounding_error,
            changes,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        entry_slot: Slot<RewardDistributorEntrySlotValue>,
    ) -> Result<(Conditions, u64), DriverError> {
        let my_state = distributor.pending_spend.latest_state.1;
        let entry_slot = distributor.actual_entry_slot_value(entry_slot);

        let withdrawal_amount_precision = u128::from(entry_slot.info.value.shares)
            * (my_state.round_reward_info.cumulative_payout
                - entry_slot.info.value.initial_cumulative_payout);
        let withdrawal_amount =
            u64::try_from(withdrawal_amount_precision / u128::from(self.precision))?;
        let payout_rounding_error = withdrawal_amount_precision % u128::from(self.precision);

        // this announcement/message should be asserted to ensure everything goes according to plan
        let announcement_or_message_data = if self.require_approval {
            RewardDistributorReceivedMessagePrefix::initiate_payout(
                withdrawal_amount,
                payout_rounding_error,
            )
        } else {
            RewardDistributorCreatedAnnouncementPrefix::initiate_payout(
                entry_slot.info.value.payout_puzzle_hash,
                withdrawal_amount,
            )
        };

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorInitiatePayoutActionSolution {
            slot_counter: entry_slot.info.value.counter,
            entry_payout_amount: withdrawal_amount,
            entry_payout_puzzle_hash: entry_slot.info.value.payout_puzzle_hash,
            entry_initial_cumulative_payout: entry_slot.info.value.initial_cumulative_payout,
            entry_shares: entry_slot.info.value.shares,
            payout_rounding_error,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        // spend entry slot
        entry_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        Ok((
            if self.require_approval {
                Conditions::new().send_message(
                    18,
                    announcement_or_message_data.into(),
                    vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
                )
            } else {
                Conditions::new().assert_puzzle_announcement(announcement_id(
                    distributor.coin.puzzle_hash,
                    announcement_or_message_data,
                ))
            },
            withdrawal_amount,
        ))
    }
}
