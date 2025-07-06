use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::{Bytes, Bytes32},
};
use chia_wallet_sdk::{
    driver::{DriverError, Spend, SpendContext},
    types::Conditions,
};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{
    Action, RewardDistributor, RewardDistributorCommitmentSlotValue, RewardDistributorConstants,
    RewardDistributorRewardSlotValue, RewardDistributorSlotNonce, Slot, SpendContextExt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardDistributorWithdrawIncentivesAction {
    pub launcher_id: Bytes32,
    pub withdrawal_share_bps: u64,
}

impl ToTreeHash for RewardDistributorWithdrawIncentivesAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorWithdrawIncentivesActionArgs::curry_tree_hash(
            self.launcher_id,
            self.withdrawal_share_bps,
        )
    }
}

impl Action<RewardDistributor> for RewardDistributorWithdrawIncentivesAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            withdrawal_share_bps: constants.withdrawal_share_bps,
        }
    }
}

impl RewardDistributorWithdrawIncentivesAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.reward_distributor_withdraw_incentives_action_puzzle()?,
            args: RewardDistributorWithdrawIncentivesActionArgs::new(
                self.launcher_id,
                self.withdrawal_share_bps,
            ),
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    pub fn created_slot_value(
        ctx: &SpendContext,
        withdrawal_share_bps: u64,
        solution: NodePtr,
    ) -> Result<RewardDistributorRewardSlotValue, DriverError> {
        let solution =
            ctx.extract::<RewardDistributorWithdrawIncentivesActionSolution>(solution)?;
        let withdrawal_share = solution.committed_value * withdrawal_share_bps / 10000;

        let new_reward_slot_value = RewardDistributorRewardSlotValue {
            epoch_start: solution.reward_slot_epoch_time,
            next_epoch_initialized: solution.reward_slot_next_epoch_initialized,
            rewards: solution.reward_slot_total_rewards - withdrawal_share,
        };

        Ok(new_reward_slot_value)
    }

    pub fn spent_slot_values(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<
        (
            RewardDistributorRewardSlotValue,
            RewardDistributorCommitmentSlotValue,
        ),
        DriverError,
    > {
        let solution =
            ctx.extract::<RewardDistributorWithdrawIncentivesActionSolution>(solution)?;

        let old_reward_slot_value = RewardDistributorRewardSlotValue {
            epoch_start: solution.reward_slot_epoch_time,
            next_epoch_initialized: solution.reward_slot_next_epoch_initialized,
            rewards: solution.reward_slot_total_rewards,
        };
        let commitment_slot_value = RewardDistributorCommitmentSlotValue {
            epoch_start: solution.reward_slot_epoch_time,
            clawback_ph: solution.clawback_ph,
            rewards: solution.committed_value,
        };

        Ok((old_reward_slot_value, commitment_slot_value))
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        commitment_slot: Slot<RewardDistributorCommitmentSlotValue>,
        reward_slot: Slot<RewardDistributorRewardSlotValue>,
    ) -> Result<(Conditions, u64), DriverError> {
        // last u64 = withdrawn amount
        let commitment_slot = distributor.actual_commitment_slot_value(commitment_slot);
        let reward_slot = distributor.actual_reward_slot_value(reward_slot);
        let withdrawal_share = commitment_slot.info.value.rewards
            * distributor.info.constants.withdrawal_share_bps
            / 10000;

        // calculate message that the withdrawer needs to send
        let withdraw_incentives_conditions = Conditions::new()
            .send_message(
                18,
                Bytes::new(Vec::new()),
                vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
            )
            .assert_concurrent_puzzle(commitment_slot.coin.puzzle_hash);

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorWithdrawIncentivesActionSolution {
            reward_slot_epoch_time: reward_slot.info.value.epoch_start,
            reward_slot_next_epoch_initialized: reward_slot.info.value.next_epoch_initialized,
            reward_slot_total_rewards: reward_slot.info.value.rewards,
            clawback_ph: commitment_slot.info.value.clawback_ph,
            committed_value: commitment_slot.info.value.rewards,
            withdrawal_share,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slots
        let my_inner_puzzle_hash: Bytes32 = distributor.info.inner_puzzle_hash().into();
        reward_slot.spend(ctx, my_inner_puzzle_hash)?;
        commitment_slot.spend(ctx, my_inner_puzzle_hash)?;

        Ok((withdraw_incentives_conditions, withdrawal_share))
    }
}

pub const REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE: [u8; 805] = hex!("ff02ffff01ff04ffff04ff4fffff04ffff11ff81afffff02ffff03ffff09ff820fdfffff05ffff14ffff12ff17ff820bdf80ffff01822710808080ffff01820fdfffff01ff088080ff018080ff81ef8080ffff04ffff04ff10ffff04ff819fff808080ffff04ffff02ff3effff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff04ff819fffff04ff82015fffff04ff8202dfff808080808080ff8080808080ffff04ffff02ff16ffff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff04ff819fffff04ff82015fffff04ffff11ff8202dfff820fdf80ff808080808080ffff04ffff0bffff0101ff819f80ff808080808080ffff04ffff02ff3effff04ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ff819fffff04ff8205dfffff04ff820bdfff808080808080ff8080808080ffff04ffff04ff14ffff04ffff0112ffff04ff80ffff04ff8205dfff8080808080ffff04ffff04ffff0181d6ffff04ff18ffff04ff8205dfffff04ff820fdfffff04ffff04ff8205dfff8080ff808080808080ff8080808080808080ffff04ffff01ffffff5533ff4342ffff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff04ff18ffff04ffff0bff5affff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12ff6affff0bffff0101ff0b8080ffff0bff12ff6aff4a808080ff4a808080ffff04ff80ffff04ffff04ff17ff8080ff8080808080ffff0bffff0102ffff0bffff0101ff0580ffff0bffff0102ffff0bffff0101ff0b80ffff0bffff0101ff17808080ff04ff1cffff04ffff0112ffff04ff80ffff04ffff0bff5affff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12ff6affff0bffff0101ff0b8080ffff0bff12ff6aff4a808080ff4a808080ff8080808080ff018080");

pub const REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    bb70077a60a28a4e262b286af3253ac52f977e1f9413b142a2efd83044a041f0
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorWithdrawIncentivesActionArgs {
    pub reward_slot_1st_curry_hash: Bytes32,
    pub commitment_slot_1st_curry_hash: Bytes32,
    pub withdrawal_share_bps: u64,
}

impl RewardDistributorWithdrawIncentivesActionArgs {
    pub fn new(launcher_id: Bytes32, withdrawal_share_bps: u64) -> Self {
        Self {
            reward_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::REWARD.to_u64(),
            )
            .into(),
            commitment_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::COMMITMENT.to_u64(),
            )
            .into(),
            withdrawal_share_bps,
        }
    }
}

impl RewardDistributorWithdrawIncentivesActionArgs {
    pub fn curry_tree_hash(launcher_id: Bytes32, withdrawal_share_bps: u64) -> TreeHash {
        CurriedProgram {
            program: REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE_HASH,
            args: RewardDistributorWithdrawIncentivesActionArgs::new(
                launcher_id,
                withdrawal_share_bps,
            ),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorWithdrawIncentivesActionSolution {
    pub reward_slot_epoch_time: u64,
    pub reward_slot_next_epoch_initialized: bool,
    pub reward_slot_total_rewards: u64,
    pub clawback_ph: Bytes32,
    pub committed_value: u64,
    #[clvm(rest)]
    pub withdrawal_share: u64,
}
