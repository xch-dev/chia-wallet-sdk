use chia_protocol::Bytes32;
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        RewardDistributorAddIncentivesActionArgs, RewardDistributorAddIncentivesActionSolution,
    },
    Conditions, Mod,
};
use clvm_traits::clvm_tuple;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, RewardDistributor, RewardDistributorConstants, SingletonAction, Spend,
    SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorAddIncentivesAction {
    pub fee_payout_puzzle_hash: Bytes32,
    pub fee_bps: u64,
}

impl ToTreeHash for RewardDistributorAddIncentivesAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorAddIncentivesActionArgs {
            fee_payout_puzzle_hash: self.fee_payout_puzzle_hash,
            fee_bps: self.fee_bps,
        }
        .curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorAddIncentivesAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            fee_payout_puzzle_hash: constants.fee_payout_puzzle_hash,
            fee_bps: constants.fee_bps,
        }
    }
}

impl RewardDistributorAddIncentivesAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(RewardDistributorAddIncentivesActionArgs {
            fee_payout_puzzle_hash: self.fee_payout_puzzle_hash,
            fee_bps: self.fee_bps,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        amount: u64,
    ) -> Result<Conditions, DriverError> {
        let my_state = distributor.pending_spend.latest_state.1;

        // calculate announcement needed to ensure everything's happening as expected
        let mut add_incentives_announcement =
            clvm_tuple!(amount, my_state.round_time_info.epoch_end)
                .tree_hash()
                .to_vec();
        add_incentives_announcement.insert(0, b'i');
        let add_incentives_announcement = Conditions::new().assert_puzzle_announcement(
            announcement_id(distributor.coin.puzzle_hash, add_incentives_announcement),
        );

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorAddIncentivesActionSolution {
            amount,
            manager_fee: amount * distributor.info.constants.fee_bps / 10000,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok(add_incentives_announcement)
    }
}
