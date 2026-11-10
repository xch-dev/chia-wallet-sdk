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

use crate::DriverError;

/// Calculates `bps` basis points of `amount`, rounded down to match the puzzles, which use
/// arbitrary precision integers. Fails with [`DriverError::TryFromInt`] if the result doesn't fit
/// in a [`u64`], which can only happen if `bps` is greater than 10,000.
fn bps_of(amount: u64, bps: u64) -> Result<u64, DriverError> {
    Ok(u64::try_from(u128::from(amount) * u128::from(bps) / 10000)?)
}

#[cfg(test)]
mod tests {
    use chia_protocol::{Bytes32, Coin};
    use chia_puzzle_types::{LineageProof, Proof};
    use chia_sdk_types::puzzles::{
        RewardDistributorCommitmentSlotValue, RewardDistributorRewardSlotValue,
        RewardDistributorSlotNonce, RewardDistributorWithdrawIncentivesActionSolution, SlotInfo,
    };
    use clvm_utils::ToTreeHash;

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
}
