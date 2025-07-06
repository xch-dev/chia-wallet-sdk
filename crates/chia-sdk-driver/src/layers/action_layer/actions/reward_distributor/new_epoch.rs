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
    Action, RewardDistributor, RewardDistributorConstants, RewardDistributorRewardSlotValue,
    RewardDistributorSlotNonce, Slot, SpendContextExt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardDistributorNewEpochAction {
    pub launcher_id: Bytes32,
    pub fee_payout_puzzle_hash: Bytes32,
    pub fee_bps: u64,
    pub epoch_seconds: u64,
}

impl ToTreeHash for RewardDistributorNewEpochAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorNewEpochAction::curry_tree_hash(
            self.launcher_id,
            self.fee_payout_puzzle_hash,
            self.fee_bps,
            self.epoch_seconds,
        )
    }
}

impl Action<RewardDistributor> for RewardDistributorNewEpochAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            fee_payout_puzzle_hash: constants.fee_payout_puzzle_hash,
            fee_bps: constants.fee_bps,
            epoch_seconds: constants.epoch_seconds,
        }
    }
}

impl RewardDistributorNewEpochAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.reward_distributor_new_epoch_action_puzzle()?,
            args: RewardDistributorNewEpochActionArgs::new(
                self.launcher_id,
                self.fee_payout_puzzle_hash,
                self.fee_bps,
                self.epoch_seconds,
            ),
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    pub fn created_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorRewardSlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorNewEpochActionSolution>(solution)?;

        Ok(RewardDistributorRewardSlotValue {
            epoch_start: solution.slot_epoch_time,
            next_epoch_initialized: solution.slot_next_epoch_initialized,
            rewards: solution.slot_total_rewards,
        })
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorRewardSlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorNewEpochActionSolution>(solution)?;

        Ok(RewardDistributorRewardSlotValue {
            epoch_start: solution.slot_epoch_time,
            next_epoch_initialized: solution.slot_next_epoch_initialized,
            rewards: solution.slot_total_rewards,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        reward_slot: Slot<RewardDistributorRewardSlotValue>,
    ) -> Result<(Conditions, u64), DriverError> {
        // also returns fee
        let my_state = distributor.pending_spend.latest_state.1;
        let reward_slot = distributor.actual_reward_slot_value(reward_slot);

        let epoch_total_rewards =
            if my_state.round_time_info.epoch_end == reward_slot.info.value.epoch_start {
                reward_slot.info.value.rewards
            } else {
                0
            };
        let fee = epoch_total_rewards * distributor.info.constants.fee_bps / 10000;

        // calculate announcement needed to ensure everything's happening as expected
        let mut new_epoch_announcement: Vec<u8> =
            my_state.round_time_info.epoch_end.tree_hash().to_vec();
        new_epoch_announcement.insert(0, b'e');
        let new_epoch_conditions = Conditions::new()
            .assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                new_epoch_announcement,
            ))
            .assert_concurrent_puzzle(reward_slot.coin.puzzle_hash);

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorNewEpochActionSolution {
            slot_epoch_time: reward_slot.info.value.epoch_start,
            slot_next_epoch_initialized: reward_slot.info.value.next_epoch_initialized,
            slot_total_rewards: reward_slot.info.value.rewards,
            epoch_total_rewards,
            fee,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        // spend slot
        reward_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok((new_epoch_conditions, fee))
    }
}

impl RewardDistributorNewEpochAction {
    pub fn curry_tree_hash(
        launcher_id: Bytes32,
        fee_payout_puzzle_hash: Bytes32,
        fee_bps: u64,
        epoch_seconds: u64,
    ) -> TreeHash {
        CurriedProgram {
            program: REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE_HASH,
            args: RewardDistributorNewEpochActionArgs::new(
                launcher_id,
                fee_payout_puzzle_hash,
                fee_bps,
                epoch_seconds,
            ),
        }
        .tree_hash()
    }
}

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE: [u8; 839] = hex!("ff02ffff01ff02ffff03ffff22ffff09ff8213dfff821bdf80ffff09ffff05ffff14ffff12ff820bbfff1780ffff018227108080ff820fbf80ffff21ffff22ffff09ff82013fff821bdf80ffff09ff820bbfff8205bf8080ffff22ffff15ff821bdfff82013f80ffff20ff8202bf80ffff09ff820bbfff8080808080ffff01ff04ffff04ff819fffff04ffff11ff82015fff820fbf80ffff04ff8202dfffff04ffff04ff8209dfffff10ff820ddfffff11ff820bbfff820fbf808080ffff04ffff04ff821bdfffff10ff821bdfff2f8080ff808080808080ffff04ffff04ff14ffff04ffff0effff0165ffff0bffff0101ff821bdf8080ff808080ffff04ffff04ffff0181d6ffff04ff08ffff04ff0bffff04ff820fbfffff04ffff04ff0bff8080ff808080808080ffff04ffff02ff1effff04ff02ffff04ff05ffff04ffff0bffff0102ffff0bffff0101ff82013f80ffff0bffff0102ffff0bffff0101ff8202bf80ffff0bffff0101ff8205bf808080ff8080808080ffff04ffff02ff16ffff04ff02ffff04ff05ffff04ffff0bffff0102ffff0bffff0101ff82013f80ffff0bffff0102ffff0bffff0101ff8202bf80ffff0bffff0101ff8205bf808080ffff04ffff0bffff0101ff82013f80ff808080808080ff808080808080ffff01ff088080ff0180ffff04ffff01ffff33ff3e42ffff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff04ff08ffff04ffff0bff5affff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12ff6affff0bffff0101ff0b8080ffff0bff12ff6aff4a808080ff4a808080ffff04ff80ffff04ffff04ff17ff8080ff8080808080ff04ff1cffff04ffff0112ffff04ff80ffff04ffff0bff5affff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12ff6affff0bffff0101ff0b8080ffff0bff12ff6aff4a808080ff4a808080ff8080808080ff018080");

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    ac01b2b3c3c137fa08662cf51e7eb28a238de85dbb8759050f39ef3dc461bfb9
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorNewEpochActionArgs {
    pub reward_slot_1st_curry_hash: Bytes32,
    pub fee_payout_puzzle_hash: Bytes32,
    pub fee_bps: u64,
    pub epoch_seconds: u64,
}

impl RewardDistributorNewEpochActionArgs {
    pub fn new(
        launcher_id: Bytes32,
        fee_payout_puzzle_hash: Bytes32,
        fee_bps: u64,
        epoch_seconds: u64,
    ) -> Self {
        Self {
            reward_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::REWARD.to_u64(),
            )
            .into(),
            fee_payout_puzzle_hash,
            fee_bps,
            epoch_seconds,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorNewEpochActionSolution {
    pub slot_epoch_time: u64,
    pub slot_next_epoch_initialized: bool,
    pub slot_total_rewards: u64,
    pub epoch_total_rewards: u64,
    #[clvm(rest)]
    pub fee: u64,
}
