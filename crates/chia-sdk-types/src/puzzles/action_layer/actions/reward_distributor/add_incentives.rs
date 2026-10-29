use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE: [u8; 247] = hex!(
    // Rue
    "
    ff02ffff03ffff22ffff22ffff15ff8203f7ff8202f780ffff15ff2fff808080
    ffff09ff3fffff13ffff12ff2fff0580ffff01822710808080ffff01ff04ffff
    04ff27ffff04ffff11ffff10ff57ff2f80ff3f80ffff04ff81b7ffff04ffff04
    ff820277ffff10ff820377ffff12ffff11ff2fff3f80ff0b808080ff8201f780
    808080ffff04ffff04ffff013effff04ffff0effff0169ffff0bffff0102ffff
    0bffff0101ff2f80ffff0bffff0101ff8202f7808080ff808080ffff04ffff04
    ffff0181d6ffff04ffff0133ffff04ff02ffff04ff3fffff04ffff04ff02ff80
    80ff808080808080ff80808080ffff01ff088080ff0180
    "
);

pub const REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    01146475a5ece9f0625beb4a82298e37dc864a3c162e7533585967517b37bee7
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorAddIncentivesActionArgs {
    pub fee_payout_puzzle_hash: Bytes32,
    pub fee_bps: u64,
    pub precision: u64,
}

impl RewardDistributorAddIncentivesActionArgs {
    pub fn curry_tree_hash(
        fee_payout_puzzle_hash: Bytes32,
        fee_bps: u64,
        precision: u64,
    ) -> TreeHash {
        CurriedProgram {
            program: REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE_HASH,
            args: RewardDistributorAddIncentivesActionArgs {
                fee_payout_puzzle_hash,
                fee_bps,
                precision,
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
