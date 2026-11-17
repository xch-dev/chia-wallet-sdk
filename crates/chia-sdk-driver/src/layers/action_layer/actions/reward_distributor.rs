mod action_log;
mod add_entry;
mod add_incentives;
mod commit_incentives;
mod initiate_payout;
mod new_epoch;
mod refresh;
mod remove_entry;
mod stake;
mod sync;
mod unstake;
mod withdraw_incentives;

pub use action_log::*;
pub use add_entry::*;
pub use add_incentives::*;
pub use commit_incentives::*;
pub use initiate_payout::*;
pub use new_epoch::*;
pub use refresh::*;
pub use remove_entry::*;
pub use stake::*;
pub use sync::*;
pub use unstake::*;
pub use withdraw_incentives::*;

use chia_sdk_types::puzzles::RewardDistributorEntryPayoutInfo;

use crate::DriverError;

/// Calculates `bps` basis points of `amount`, rounded down to match the puzzles, which use
/// arbitrary precision integers. Fails with [`DriverError::TryFromInt`] if the result doesn't fit
/// in a [`u64`], which can only happen if `bps` is greater than 10,000.
fn bps_of(amount: u64, bps: u64) -> Result<u64, DriverError> {
    Ok(u64::try_from(u128::from(amount) * u128::from(bps) / 10000)?)
}

/// Adds two values, failing with [`DriverError::ArithmeticOverflow`] instead of wrapping.
fn checked_add(a: u64, b: u64) -> Result<u64, DriverError> {
    a.checked_add(b).ok_or(DriverError::ArithmeticOverflow)
}

/// Subtracts `b` from `a`, failing with [`DriverError::ArithmeticOverflow`] instead of wrapping.
fn checked_sub(a: u64, b: u64) -> Result<u64, DriverError> {
    a.checked_sub(b).ok_or(DriverError::ArithmeticOverflow)
}

/// Calculates the payout owed to `shares` shares of an entry, which is
/// `shares * (cumulative_payout - initial_cumulative_payout) / precision` rounded down, and the
/// rounding error, as the puzzles do.
///
/// Fails with [`DriverError::ArithmeticOverflow`] if `initial_cumulative_payout` is greater than
/// `cumulative_payout`, the product doesn't fit in a [`u128`], or `precision` is zero, and with
/// [`DriverError::TryFromInt`] if the payout doesn't fit in a [`u64`].
fn entry_payout(
    shares: u64,
    cumulative_payout: u128,
    initial_cumulative_payout: u128,
    precision: u64,
) -> Result<RewardDistributorEntryPayoutInfo, DriverError> {
    let precision = u128::from(precision);

    if precision == 0 {
        return Err(DriverError::ArithmeticOverflow);
    }

    let payout_precision = cumulative_payout
        .checked_sub(initial_cumulative_payout)
        .and_then(|payout_per_share| payout_per_share.checked_mul(u128::from(shares)))
        .ok_or(DriverError::ArithmeticOverflow)?;

    Ok(RewardDistributorEntryPayoutInfo {
        payout_amount: u64::try_from(payout_precision / precision)?,
        payout_rounding_error: payout_precision % precision,
    })
}

#[cfg(test)]
mod tests {
    use chia_protocol::{Bytes32, Coin};
    use chia_puzzle_types::{LineageProof, Proof};
    use chia_sdk_types::puzzles::{
        RewardDistributorCatLockingPuzzleSolution, RewardDistributorCatUnlockingPuzzleSolution,
        RewardDistributorCommitIncentivesActionSolution, RewardDistributorCommitmentSlotValue,
        RewardDistributorDlInfo, RewardDistributorEntryPayoutInfo, RewardDistributorEntrySlotValue,
        RewardDistributorInitiatePayoutActionSolution, RewardDistributorNewEpochActionSolution,
        RewardDistributorRefreshNftsFromDlActionSolution, RewardDistributorRefreshNftsTotals,
        RewardDistributorRewardSlotValue, RewardDistributorSlotNonce,
        RewardDistributorStakeActionSolution, RewardDistributorUnstakeActionSolution,
        RewardDistributorWithdrawIncentivesActionSolution, SlotAndNfts, SlotInfo,
    };
    use clvm_utils::ToTreeHash;
    use clvmr::NodePtr;

    use crate::{
        DriverError, Reserve, RewardDistributor, RewardDistributorConstants, RewardDistributorInfo,
        RewardDistributorState, RewardDistributorStateTransition, RewardDistributorType,
        RoundRewardInfo, RoundTimeInfo, Slot, SpendContext,
    };

    use super::*;

    const EPOCH_START: u64 = 1000;

    // Half of `u64::MAX`, rounded down, as the puzzle computes it.
    const HALF: u64 = u64::MAX / 2;

    fn lineage_proof() -> LineageProof {
        LineageProof {
            parent_parent_coin_info: Bytes32::default(),
            parent_inner_puzzle_hash: Bytes32::default(),
            parent_amount: 1,
        }
    }

    fn distributor(state: RewardDistributorState, bps: u64) -> RewardDistributor {
        let constants = RewardDistributorConstants::without_launcher_id(
            RewardDistributorType::Managed {
                manager_singleton_launcher_id: Bytes32::new([1; 32]),
            },
            Bytes32::new([2; 32]),
            100,
            1,
            0,
            1,
            false,
            bps,
            bps,
            Bytes32::new([3; 32]),
        )
        .with_launcher_id(Bytes32::new([4; 32]));

        RewardDistributor::new(
            Coin::new(Bytes32::default(), Bytes32::default(), 1),
            Proof::Lineage(lineage_proof()),
            RewardDistributorInfo::new(state, constants),
            Reserve::new(
                Bytes32::default(),
                lineage_proof(),
                constants.reserve_asset_id,
                Bytes32::default(),
                0,
                state.total_reserves,
            ),
        )
    }

    fn state(total_reserves: u64, last_update: u64, epoch_end: u64) -> RewardDistributorState {
        RewardDistributorState {
            total_reserves,
            active_shares: 0,
            round_reward_info: RoundRewardInfo {
                cumulative_payout: 0,
                remaining_rewards: 0,
            },
            round_time_info: RoundTimeInfo {
                last_update,
                epoch_end,
            },
        }
    }

    fn slot<V: ToTreeHash>(distributor: &RewardDistributor, nonce: u64, value: V) -> Slot<V> {
        Slot::new(
            lineage_proof(),
            SlotInfo::from_value(distributor.info.constants.launcher_id, nonce, value),
        )
    }

    fn reward_slot(distributor: &RewardDistributor) -> Slot<RewardDistributorRewardSlotValue> {
        slot(
            distributor,
            RewardDistributorSlotNonce::REWARD.to_u64(),
            RewardDistributorRewardSlotValue {
                counter: 0,
                epoch_start: EPOCH_START,
                next_epoch_initialized: false,
                rewards: u64::MAX,
            },
        )
    }

    #[test]
    fn test_add_incentives_fee_does_not_overflow() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let mut distributor = distributor(state(0, 0, EPOCH_START), 5000);

        let action = distributor.new_action::<RewardDistributorAddIncentivesAction>();
        let _conditions = action.spend(ctx, &mut distributor, u64::MAX)?;

        assert_eq!(
            distributor.pending_spend.latest_state.1.total_reserves,
            u64::MAX - HALF
        );

        Ok(())
    }

    #[test]
    fn test_new_epoch_fee_does_not_overflow() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let mut distributor = distributor(state(u64::MAX, EPOCH_START, EPOCH_START), 5000);

        let reward_slot = reward_slot(&distributor);
        let action = distributor.new_action::<RewardDistributorNewEpochAction>();
        let (_, fee) = action.spend(ctx, &mut distributor, reward_slot)?;

        assert_eq!(fee, HALF);
        assert_eq!(
            distributor.pending_spend.latest_state.1.total_reserves,
            u64::MAX - HALF
        );

        Ok(())
    }

    #[test]
    fn test_withdraw_incentives_share_does_not_overflow() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let mut distributor = distributor(state(u64::MAX, 0, 0), 5000);

        let reward_slot = reward_slot(&distributor);
        let commitment_slot = slot(
            &distributor,
            RewardDistributorSlotNonce::COMMITMENT.to_u64(),
            RewardDistributorCommitmentSlotValue {
                epoch_start: EPOCH_START,
                clawback_ph: Bytes32::new([5; 32]),
                rewards: u64::MAX,
            },
        );
        let action = distributor.new_action::<RewardDistributorWithdrawIncentivesAction>();
        let (_, withdrawal_share) =
            action.spend(ctx, &mut distributor, commitment_slot, reward_slot)?;

        assert_eq!(withdrawal_share, HALF);
        assert_eq!(
            distributor.pending_spend.latest_state.1.total_reserves,
            u64::MAX - HALF
        );

        let RewardDistributorActionLog::WithdrawIncentives(log) = distributor.pending_spend.logs[0]
        else {
            panic!("expected a withdraw incentives log");
        };
        assert_eq!(log.created_reward_slot.rewards, u64::MAX - HALF);

        Ok(())
    }

    #[test]
    fn test_withdraw_incentives_share_that_does_not_fit_is_an_error() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let solution = ctx.alloc(&RewardDistributorWithdrawIncentivesActionSolution {
            reward_slot_counter: 0,
            reward_slot_epoch_time: EPOCH_START,
            clawback_ph: Bytes32::default(),
            committed_value: u64::MAX,
            reward_slot_total_rewards: u64::MAX,
            reward_slot_next_epoch_initialized: false,
        })?;
        let state = state(u64::MAX, 0, 0);
        let changes = RewardDistributorStateTransition {
            old_state: state,
            new_state: state,
        };

        assert!(
            RewardDistributorWithdrawIncentivesAction::get_log(ctx, solution, changes, 20_000)
                .is_err_and(|error| matches!(error, DriverError::TryFromInt(_)))
        );

        Ok(())
    }

    fn no_changes() -> RewardDistributorStateTransition {
        let state = state(0, 0, 0);
        RewardDistributorStateTransition {
            old_state: state,
            new_state: state,
        }
    }

    fn is_overflow<T>(result: Result<T, DriverError>) -> bool {
        result.is_err_and(|error| matches!(error, DriverError::ArithmeticOverflow))
    }

    fn is_invalid_epoch<T>(result: Result<T, DriverError>) -> bool {
        result.is_err_and(|error| matches!(error, DriverError::InvalidRewardDistributorEpoch))
    }

    fn withdraw_log(
        reward_slot_counter: u64,
        committed_value: u64,
        reward_slot_total_rewards: u64,
        withdrawal_share_bps: u64,
    ) -> Result<RewardDistributorWithdrawIncentivesActionLog, DriverError> {
        let ctx = &mut SpendContext::new();
        let solution = ctx.alloc(&RewardDistributorWithdrawIncentivesActionSolution {
            reward_slot_counter,
            reward_slot_epoch_time: EPOCH_START,
            clawback_ph: Bytes32::default(),
            committed_value,
            reward_slot_total_rewards,
            reward_slot_next_epoch_initialized: false,
        })?;
        RewardDistributorWithdrawIncentivesAction::get_log(
            ctx,
            solution,
            no_changes(),
            withdrawal_share_bps,
        )
    }

    #[test]
    fn test_withdraw_incentives_share_over_slot_rewards_is_an_error() -> anyhow::Result<()> {
        // The slot holds less than the withdrawal share, which would underflow.
        assert!(is_overflow(withdraw_log(0, 1000, 99, 1000)));

        // Exactly the slot's rewards is fine.
        let log = withdraw_log(0, 1000, 100, 1000)?;
        assert_eq!(log.created_reward_slot.rewards, 0);
        assert_eq!(log.created_reward_slot.counter, 1);

        Ok(())
    }

    #[test]
    fn test_withdraw_incentives_counter_overflow_is_an_error() {
        assert!(is_overflow(withdraw_log(u64::MAX, 0, 0, 1000)));
    }

    fn commit_log(
        solution: RewardDistributorCommitIncentivesActionSolution,
        epoch_seconds: u64,
    ) -> Result<RewardDistributorCommitIncentivesActionLog, DriverError> {
        let ctx = &mut SpendContext::new();
        let solution = ctx.alloc(&solution)?;
        RewardDistributorCommitIncentivesAction::get_log(ctx, solution, no_changes(), epoch_seconds)
    }

    fn commit_solution(
        slot_epoch_time: u64,
        epoch_start: u64,
    ) -> RewardDistributorCommitIncentivesActionSolution {
        RewardDistributorCommitIncentivesActionSolution {
            slot_counter: 0,
            slot_epoch_time,
            slot_next_epoch_initialized: false,
            slot_total_rewards: 0,
            epoch_start,
            clawback_ph: Bytes32::default(),
            rewards_to_add: 7,
        }
    }

    #[test]
    fn test_commit_incentives_rewards_overflow_is_an_error() -> anyhow::Result<()> {
        let mut solution = commit_solution(EPOCH_START, EPOCH_START);
        solution.slot_total_rewards = u64::MAX - 6;
        assert!(is_overflow(commit_log(solution, 100)));

        solution.slot_total_rewards = u64::MAX - 7;
        let log = commit_log(solution, 100)?;
        assert_eq!(log.created_reward_slots[0].rewards, u64::MAX);

        Ok(())
    }

    #[test]
    fn test_commit_incentives_counter_overflow_is_an_error() {
        let mut solution = commit_solution(EPOCH_START, EPOCH_START);
        solution.slot_counter = u64::MAX;
        assert!(is_overflow(commit_log(solution, 100)));

        let mut solution = commit_solution(EPOCH_START, EPOCH_START + 100);
        solution.slot_counter = u64::MAX;
        assert!(is_overflow(commit_log(solution, 100)));
    }

    #[test]
    fn test_commit_incentives_creates_slots_up_to_the_target_epoch() -> anyhow::Result<()> {
        let mut solution = commit_solution(EPOCH_START, EPOCH_START + 300);
        solution.slot_counter = 4;
        solution.slot_total_rewards = 50;
        let log = commit_log(solution, 100)?;

        let slot = |counter, epoch_start, next_epoch_initialized, rewards| {
            RewardDistributorRewardSlotValue {
                counter,
                epoch_start,
                next_epoch_initialized,
                rewards,
            }
        };
        assert_eq!(
            log.created_reward_slots,
            vec![
                slot(5, EPOCH_START, true, 50),
                slot(0, EPOCH_START + 300, false, 7),
                slot(0, EPOCH_START + 100, true, 0),
                slot(0, EPOCH_START + 200, true, 0),
            ]
        );

        // The next epoch needs no slots in between.
        let log = commit_log(commit_solution(EPOCH_START, EPOCH_START + 100), 100)?;
        assert_eq!(log.created_reward_slots.len(), 2);

        Ok(())
    }

    #[test]
    fn test_commit_incentives_zero_epoch_seconds_is_an_error() -> anyhow::Result<()> {
        // This used to loop forever.
        assert!(is_invalid_epoch(commit_log(
            commit_solution(EPOCH_START, EPOCH_START + 1),
            0
        )));

        // Committing to the spent slot's own epoch doesn't use the epoch length, and the puzzle
        // allows it.
        let log = commit_log(commit_solution(EPOCH_START, EPOCH_START), 0)?;
        assert_eq!(log.created_reward_slots.len(), 1);

        Ok(())
    }

    #[test]
    fn test_commit_incentives_unreachable_epoch_is_an_error() {
        // Before the spent slot's epoch.
        assert!(is_invalid_epoch(commit_log(
            commit_solution(EPOCH_START, EPOCH_START - 100),
            100
        )));

        // Not a whole number of epochs after it.
        assert!(is_invalid_epoch(commit_log(
            commit_solution(EPOCH_START, EPOCH_START + 150),
            100
        )));

        // Stepping past `u64::MAX` used to overflow.
        assert!(is_invalid_epoch(commit_log(
            commit_solution(u64::MAX - 5, u64::MAX),
            10
        )));
    }

    #[test]
    fn test_commit_incentives_near_u64_max_is_parsed() -> anyhow::Result<()> {
        let log = commit_log(commit_solution(u64::MAX - 10, u64::MAX), 5)?;
        assert_eq!(
            log.created_reward_slots
                .iter()
                .map(|slot| slot.epoch_start)
                .collect::<Vec<_>>(),
            vec![u64::MAX - 10, u64::MAX, u64::MAX - 5]
        );
        Ok(())
    }

    #[test]
    fn test_commit_incentives_slot_count_is_bounded() -> anyhow::Result<()> {
        // 8,148 `CREATE_COIN`s fit in a block, and two of them are the updated reward slot and
        // the commitment slot.
        let log = commit_log(commit_solution(0, 8146), 1)?;
        assert_eq!(log.created_reward_slots.len(), 8147);

        assert!(is_invalid_epoch(commit_log(commit_solution(0, 8147), 1)));

        // This used to push `u64::MAX` slots.
        assert!(is_invalid_epoch(commit_log(
            commit_solution(0, u64::MAX),
            1
        )));

        Ok(())
    }

    #[test]
    fn test_commit_incentives_spend_with_unreachable_epoch_is_an_error() {
        let ctx = &mut SpendContext::new();
        let mut distributor = distributor(state(0, 0, EPOCH_START), 5000);
        let reward_slot = reward_slot(&distributor);

        let action = distributor.new_action::<RewardDistributorCommitIncentivesAction>();
        assert!(is_invalid_epoch(action.spend(
            ctx,
            &mut distributor,
            reward_slot,
            EPOCH_START + 150,
            Bytes32::default(),
            1,
        )));

        // The failed spend doesn't leave the reward slot's spend behind.
        assert!(ctx.take().is_empty());
        assert!(distributor.pending_spend.logs.is_empty());
    }

    #[test]
    fn test_entry_payout() -> anyhow::Result<()> {
        // 3 shares * (1000 - 250) / 7 = 321 remainder 3
        let payout = entry_payout(3, 1000, 250, 7)?;
        assert_eq!(payout.payout_amount, 321);
        assert_eq!(payout.payout_rounding_error, 3);

        // The entry's initial cumulative payout is after the current one.
        assert!(is_overflow(entry_payout(1, 250, 1000, 7)));

        // The product doesn't fit in a `u128`.
        assert!(is_overflow(entry_payout(u64::MAX, u128::MAX, 0, 7)));

        // Zero precision used to divide by zero.
        assert!(is_overflow(entry_payout(1, 1, 0, 0)));

        // The payout doesn't fit in a `u64`.
        assert!(
            entry_payout(u64::MAX, 2, 0, 1)
                .is_err_and(|error| matches!(error, DriverError::TryFromInt(_)))
        );

        Ok(())
    }

    #[test]
    fn test_new_epoch_counter_overflow_is_an_error() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let solution = ctx.alloc(&RewardDistributorNewEpochActionSolution {
            slot_counter: u64::MAX,
            slot_epoch_time: EPOCH_START,
            slot_next_epoch_initialized: false,
            slot_total_rewards: 0,
            epoch_total_rewards: 0,
        })?;
        assert!(is_overflow(RewardDistributorNewEpochAction::get_log(
            ctx,
            solution,
            no_changes()
        )));
        Ok(())
    }

    #[test]
    fn test_initiate_payout_counter_overflow_is_an_error() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let solution = ctx.alloc(&RewardDistributorInitiatePayoutActionSolution {
            slot_counter: u64::MAX,
            entry_payout_amount: 0,
            entry_payout_puzzle_hash: Bytes32::default(),
            payout_rounding_error: 0,
            entry_shares: 0,
            entry_initial_cumulative_payout: 0,
        })?;
        assert!(is_overflow(RewardDistributorInitiatePayoutAction::get_log(
            ctx,
            solution,
            no_changes()
        )));
        Ok(())
    }

    fn entry_slot(counter: u64, shares: u64) -> RewardDistributorEntrySlotValue {
        RewardDistributorEntrySlotValue {
            counter,
            payout_puzzle_hash: Bytes32::new([5; 32]),
            initial_cumulative_payout: 0,
            shares,
        }
    }

    #[test]
    fn test_refresh_counter_overflow_is_an_error() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let solution = ctx.alloc(&RewardDistributorRefreshNftsFromDlActionSolution {
            dl_root_hash: Bytes32::default(),
            dl_info: RewardDistributorDlInfo {
                dl_metadata_rest_hash: None,
                dl_metadata_updater_hash_hash: Bytes32::default(),
                dl_inner_puzzle_hash: Bytes32::default(),
            },
            totals: RewardDistributorRefreshNftsTotals {
                total_entry_payout_amount: 0,
                total_shares_delta: 0,
                total_payout_rounding_error: 0,
            },
            slots_and_nfts: vec![SlotAndNfts {
                existing_slot_value: entry_slot(u64::MAX, 1),
                entry_payout_info: RewardDistributorEntryPayoutInfo {
                    payout_amount: 0,
                    payout_rounding_error: 0,
                },
                nfts_total_shares_delta: 0,
                nfts: vec![],
            }],
        })?;
        assert!(is_overflow(RewardDistributorRefreshAction::get_log(
            ctx,
            solution,
            no_changes(),
            Bytes32::default()
        )));
        Ok(())
    }

    const CAT_TYPE: RewardDistributorType = RewardDistributorType::Cat {
        asset_id: Bytes32::new([6; 32]),
        hidden_puzzle_hash: None,
    };

    fn stake_log(
        existing_slot_counter: i128,
        existing_slot_shares: u64,
        cat_amount: u64,
    ) -> Result<RewardDistributorStakeActionLog, DriverError> {
        let ctx = &mut SpendContext::new();
        let lock_puzzle_solution = ctx.alloc(&RewardDistributorCatLockingPuzzleSolution {
            my_id: Bytes32::default(),
            cat_amount,
            cat_maker_solution_rest: (),
        })?;
        let solution = ctx.alloc(&RewardDistributorStakeActionSolution {
            lock_puzzle_solution,
            existing_slot_counter,
            entry_custody_puzzle_hash: Bytes32::new([5; 32]),
            existing_slot_cumulative_payout: 0,
            existing_slot_shares,
        })?;
        RewardDistributorStakeAction::get_log(ctx, solution, no_changes(), CAT_TYPE)
    }

    #[test]
    fn test_stake_shares_overflow_is_an_error() -> anyhow::Result<()> {
        assert!(is_overflow(stake_log(0, u64::MAX, 1)));

        let log = stake_log(0, u64::MAX - 1, 1)?;
        assert_eq!(log.created_entry_slot.shares, u64::MAX);
        assert_eq!(log.created_entry_slot.counter, 1);

        Ok(())
    }

    #[test]
    fn test_stake_counter_overflow_is_an_error() -> anyhow::Result<()> {
        // `get_log` rejects a counter that doesn't fit in a `u64` before this, but
        // `created_slot_value` is public.
        let ctx = &mut SpendContext::new();
        let lock_puzzle_solution = ctx.alloc(&RewardDistributorCatLockingPuzzleSolution {
            my_id: Bytes32::default(),
            cat_amount: 1,
            cat_maker_solution_rest: (),
        })?;
        let solution = RewardDistributorStakeActionSolution {
            lock_puzzle_solution,
            existing_slot_counter: i128::MAX,
            entry_custody_puzzle_hash: Bytes32::new([5; 32]),
            existing_slot_cumulative_payout: 0,
            existing_slot_shares: 0,
        };
        assert!(is_overflow(
            RewardDistributorStakeAction::created_slot_value(
                ctx,
                &state(0, 0, 0),
                CAT_TYPE,
                &solution
            )
        ));
        assert!(stake_log(i128::MAX, 0, 1).is_err());
        Ok(())
    }

    fn unstake_log(
        entry_slot: RewardDistributorEntrySlotValue,
        cat_shares: u64,
    ) -> Result<RewardDistributorUnstakeActionLog, DriverError> {
        let ctx = &mut SpendContext::new();
        let solution = ctx.alloc(&RewardDistributorUnstakeActionSolution {
            entry_slot,
            entry_payout_info: RewardDistributorEntryPayoutInfo {
                payout_amount: 0,
                payout_rounding_error: 0,
            },
            unlock_puzzle_solution: RewardDistributorCatUnlockingPuzzleSolution {
                cat_parent_id: Bytes32::default(),
                cat_amount: cat_shares,
                cat_shares,
                cat_maker_solution_rest: (),
            },
        })?;
        RewardDistributorUnstakeAction::get_log(
            ctx,
            solution,
            no_changes(),
            Bytes32::new([4; 32]),
            CAT_TYPE,
            NodePtr::NIL,
        )
    }

    #[test]
    fn test_unstake_more_shares_than_the_entry_is_an_error() -> anyhow::Result<()> {
        assert!(is_overflow(unstake_log(entry_slot(0, 4), 5)));

        let log = unstake_log(entry_slot(0, 5), 5)?;
        assert_eq!(log.created_entry_slot.shares, 0);

        Ok(())
    }

    #[test]
    fn test_unstake_counter_overflow_is_an_error() {
        assert!(is_overflow(unstake_log(entry_slot(u64::MAX, 5), 5)));
    }
}
