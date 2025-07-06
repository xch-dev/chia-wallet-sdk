use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
};
use chia_wallet_sdk::{
    driver::{DriverError, Spend, SpendContext},
    types::{announcement_id, Conditions},
};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{
    Action, RewardDistributor, RewardDistributorCommitmentSlotValue, RewardDistributorConstants,
    RewardDistributorRewardSlotValue, RewardDistributorSlotNonce, Slot, SpendContextExt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardDistributorCommitIncentivesAction {
    pub launcher_id: Bytes32,
    pub epoch_seconds: u64,
}

impl ToTreeHash for RewardDistributorCommitIncentivesAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorCommitIncentivesActionArgs::curry_tree_hash(
            self.launcher_id,
            self.epoch_seconds,
        )
    }
}

impl Action<RewardDistributor> for RewardDistributorCommitIncentivesAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            epoch_seconds: constants.epoch_seconds,
        }
    }
}

impl RewardDistributorCommitIncentivesAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.reward_distributor_commit_incentives_action_puzzle()?,
            args: RewardDistributorCommitIncentivesActionArgs::new(
                self.launcher_id,
                self.epoch_seconds,
            ),
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    pub fn created_slot_values(
        ctx: &SpendContext,
        epoch_seconds: u64,
        solution: NodePtr,
    ) -> Result<
        (
            RewardDistributorCommitmentSlotValue,
            Vec<RewardDistributorRewardSlotValue>,
        ),
        DriverError,
    > {
        let solution = ctx.extract::<RewardDistributorCommitIncentivesActionSolution>(solution)?;

        let commitment_slot_value = RewardDistributorCommitmentSlotValue {
            epoch_start: solution.epoch_start,
            clawback_ph: solution.clawback_ph,
            rewards: solution.rewards_to_add,
        };

        let mut reward_slot_values: Vec<RewardDistributorRewardSlotValue> = vec![];

        if solution.slot_epoch_time == solution.epoch_start {
            reward_slot_values.push(RewardDistributorRewardSlotValue {
                epoch_start: solution.epoch_start,
                next_epoch_initialized: solution.slot_next_epoch_initialized,
                rewards: solution.slot_total_rewards + solution.rewards_to_add,
            })
        } else {
            reward_slot_values.push(RewardDistributorRewardSlotValue {
                epoch_start: solution.slot_epoch_time,
                next_epoch_initialized: true,
                rewards: solution.slot_total_rewards,
            });
            reward_slot_values.push(RewardDistributorRewardSlotValue {
                epoch_start: solution.epoch_start,
                next_epoch_initialized: false,
                rewards: solution.rewards_to_add,
            });

            let mut start_epoch_time = solution.slot_epoch_time + epoch_seconds;
            let end_epoch_time = solution.epoch_start;
            while end_epoch_time > start_epoch_time {
                reward_slot_values.push(RewardDistributorRewardSlotValue {
                    epoch_start: start_epoch_time,
                    next_epoch_initialized: true,
                    rewards: 0,
                });

                start_epoch_time += epoch_seconds;
            }
        }

        Ok((commitment_slot_value, reward_slot_values))
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorRewardSlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorCommitIncentivesActionSolution>(solution)?;

        Ok(RewardDistributorRewardSlotValue {
            epoch_start: solution.slot_epoch_time,
            next_epoch_initialized: solution.slot_next_epoch_initialized,
            rewards: solution.slot_total_rewards,
        })
    }

    #[allow(clippy::type_complexity)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        reward_slot: Slot<RewardDistributorRewardSlotValue>,
        epoch_start: u64,
        clawback_ph: Bytes32,
        rewards_to_add: u64,
    ) -> Result<Conditions, DriverError> {
        let reward_slot = distributor.actual_reward_slot_value(reward_slot);

        let new_commitment_slot_value = RewardDistributorCommitmentSlotValue {
            epoch_start,
            clawback_ph,
            rewards: rewards_to_add,
        };

        // calculate announcement
        let mut commit_reward_announcement: Vec<u8> =
            new_commitment_slot_value.tree_hash().to_vec();
        commit_reward_announcement.insert(0, b'c');

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorCommitIncentivesActionSolution {
            slot_epoch_time: reward_slot.info.value.epoch_start,
            slot_next_epoch_initialized: reward_slot.info.value.next_epoch_initialized,
            slot_total_rewards: reward_slot.info.value.rewards,
            epoch_start,
            clawback_ph,
            rewards_to_add,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        // spend reward slot
        reward_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok(
            Conditions::new().assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                commit_reward_announcement,
            )),
        )
    }
}

pub const REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE: [u8; 1209] = hex!("ff02ffff01ff02ffff03ffff22ffff20ffff15ff820defff8205df8080ffff15ff820fdfff808080ffff01ff04ffff04ff4fffff04ffff10ff81afff820fdf80ffff04ff82016fffff04ff8202efffff04ff8205efff808080808080ffff02ff12ffff04ff02ffff04ff0bffff04ffff0bffff0102ffff0bffff0101ff8205df80ffff0bffff0102ffff0bffff0101ff820bdf80ffff0bffff0101ff820fdf808080ffff04ff820bdfffff04ffff04ffff02ff3effff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff819fffff04ff82015fffff04ff8202dfff808080808080ff8080808080ffff02ffff03ffff09ff8205dfff819f80ffff01ff04ffff02ff1affff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff819fffff04ff82015fffff04ffff10ff8202dfff820fdf80ff808080808080ffff04ffff0bffff0101ff819f80ff808080808080ff8080ffff01ff02ffff03ff82015fffff01ff0880ffff01ff04ffff02ff1affff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff819fffff04ffff0101ffff04ff8202dfff808080808080ffff04ffff0bffff0101ff819f80ff808080808080ffff04ffff02ff1affff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff8205dfffff04ff80ffff04ff820fdfff808080808080ffff04ffff0bffff0101ff8205df80ff808080808080ffff02ff2effff04ff02ffff04ff05ffff04ff17ffff04ffff10ff819fff1780ffff04ff8205dfff80808080808080808080ff018080ff018080ff8080808080808080ffff01ff088080ff0180ffff04ffff01ffffff333eff42ff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffffff04ffff04ff18ffff04ffff0effff0163ff0b80ff808080ffff04ffff02ff1affff04ff02ffff04ff05ffff04ff0bffff04ff17ff808080808080ff2f8080ff04ff10ffff04ffff0bff81bcffff0bff2cffff0bff2cff81dcff0580ffff0bff2cffff0bff81fcffff0bff2cffff0bff2cff81dcffff0bffff0101ff0b8080ffff0bff2cff81dcff819c808080ff819c808080ffff04ff80ffff04ffff04ff17ff8080ff8080808080ffff0bffff0102ffff0bffff0101ff0580ffff0bffff0102ffff0bffff0101ff0b80ffff0bffff0101ff17808080ffff02ffff03ffff09ff17ff2f80ff80ffff01ff04ffff02ff1affff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff17ffff01ff01ff8080808080ffff04ffff0bffff0101ff1780ff808080808080ffff02ff2effff04ff02ffff04ff05ffff04ff0bffff04ffff10ff17ff0b80ffff04ff2fff808080808080808080ff0180ff04ff14ffff04ffff0112ffff04ff80ffff04ffff0bff81bcffff0bff2cffff0bff2cff81dcff0580ffff0bff2cffff0bff81fcffff0bff2cffff0bff2cff81dcffff0bffff0101ff0b8080ffff0bff2cff81dcff819c808080ff819c808080ff8080808080ff018080");

pub const REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    2c49bc36a8ec2f2703fddf92e4ae3dcbed849bb07cf6d3264f6714d04413acc0
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorCommitIncentivesActionArgs {
    pub reward_slot_1st_curry_hash: Bytes32,
    pub commitment_slot_1st_curry_hash: Bytes32,
    pub epoch_seconds: u64,
}

impl RewardDistributorCommitIncentivesActionArgs {
    pub fn new(launcher_id: Bytes32, epoch_seconds: u64) -> Self {
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
            epoch_seconds,
        }
    }
}

impl RewardDistributorCommitIncentivesActionArgs {
    pub fn curry_tree_hash(launcher_id: Bytes32, epoch_seconds: u64) -> TreeHash {
        CurriedProgram {
            program: REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE_HASH,
            args: RewardDistributorCommitIncentivesActionArgs::new(launcher_id, epoch_seconds),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorCommitIncentivesActionSolution {
    pub slot_epoch_time: u64,
    pub slot_next_epoch_initialized: bool,
    pub slot_total_rewards: u64,
    pub epoch_start: u64,
    pub clawback_ph: Bytes32,
    #[clvm(rest)]
    pub rewards_to_add: u64,
}
