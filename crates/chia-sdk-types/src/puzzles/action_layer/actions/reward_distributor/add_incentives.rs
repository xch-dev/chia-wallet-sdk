use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE: [u8; 261] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff15ff8206f7ff8204f780ffff15ff4fff80
    80ffff09ff6fffff05ffff14ffff12ff4fff0b80ffff0182271080808080ffff
    01ff04ffff04ff27ffff04ffff10ff57ffff11ff4fff6f8080ffff04ff81b7ff
    ff04ffff04ff820277ffff10ff820377ffff11ff4fff6f808080ffff04ff8202
    f7ff808080808080ffff04ffff04ff06ffff04ffff0effff0169ffff0bffff01
    02ffff0bffff0101ff4f80ffff0bffff0101ff8206f7808080ff808080ffff04
    ffff04ffff0181d6ffff04ff04ffff04ff05ffff04ff6fffff04ffff04ff05ff
    8080ff808080808080ff80808080ffff01ff088080ff0180ffff04ffff01ff33
    3eff018080
    "
);

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

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorAddIncentivesActionSolution {
    pub amount: u64,
    #[clvm(rest)]
    pub manager_fee: u64,
}

impl Mod for RewardDistributorAddIncentivesActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE_HASH
    }
}
