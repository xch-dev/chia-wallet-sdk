use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
};
use chia_wallet_sdk::{
    driver::{DriverError, Spend, SpendContext},
    types::{announcement_id, Conditions},
};
use clvm_traits::{clvm_tuple, FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{Action, RewardDistributor, RewardDistributorConstants, SpendContextExt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardDistributorAddIncentivesAction {
    pub fee_payout_puzzle_hash: Bytes32,
    pub fee_bps: u64,
}

impl ToTreeHash for RewardDistributorAddIncentivesAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorAddIncentivesActionArgs::curry_tree_hash(
            self.fee_payout_puzzle_hash,
            self.fee_bps,
        )
    }
}

impl Action<RewardDistributor> for RewardDistributorAddIncentivesAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            fee_payout_puzzle_hash: constants.fee_payout_puzzle_hash,
            fee_bps: constants.fee_bps,
        }
    }
}

impl RewardDistributorAddIncentivesAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.reward_distributor_add_incentives_action_puzzle()?,
            args: RewardDistributorAddIncentivesActionArgs {
                fee_payout_puzzle_hash: self.fee_payout_puzzle_hash,
                fee_bps: self.fee_bps,
            },
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        amount: u64,
    ) -> Result<Conditions, DriverError> {
        let my_state = distributor.pending_spend.latest_state.1;

        // calculate announcement needed to ensure everything's happening as expected
        let mut add_incentives_announcement: Vec<u8> =
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

pub const REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE: [u8; 261] = hex!("ff02ffff01ff02ffff03ffff22ffff15ff8206f7ff8204f780ffff15ff4fff8080ffff09ff6fffff05ffff14ffff12ff4fff0b80ffff0182271080808080ffff01ff04ffff04ff27ffff04ffff10ff57ffff11ff4fff6f8080ffff04ff81b7ffff04ffff04ff820277ffff10ff820377ffff11ff4fff6f808080ffff04ff8202f7ff808080808080ffff04ffff04ff06ffff04ffff0effff0169ffff0bffff0102ffff0bffff0101ff4f80ffff0bffff0101ff8206f7808080ff808080ffff04ffff04ffff0181d6ffff04ff04ffff04ff05ffff04ff6fffff04ffff04ff05ff8080ff808080808080ff80808080ffff01ff088080ff0180ffff04ffff01ff333eff018080");

pub const REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    eb999158d98d1013b072b7443acca10a1bdfef2eab824ea25f0d71e2e30cec7e
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorAddIncentivesActionArgs {
    pub fee_payout_puzzle_hash: Bytes32,
    pub fee_bps: u64,
}

impl RewardDistributorAddIncentivesActionArgs {
    pub fn curry_tree_hash(fee_payout_puzzle_hash: Bytes32, fee_bps: u64) -> TreeHash {
        CurriedProgram {
            program: REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE_HASH,
            args: RewardDistributorAddIncentivesActionArgs {
                fee_payout_puzzle_hash,
                fee_bps,
            },
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorAddIncentivesActionSolution {
    pub amount: u64,
    #[clvm(rest)]
    pub manager_fee: u64,
}
