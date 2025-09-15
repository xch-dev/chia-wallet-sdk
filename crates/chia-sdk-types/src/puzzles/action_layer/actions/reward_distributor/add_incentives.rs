use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE: [u8; 278] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff15ff820defff8209ef80ffff15ff819fff
    8080ffff09ff81dfffff05ffff14ffff12ff819fff0b80ffff01822710808080
    80ffff01ff04ffff04ff4fffff04ffff10ff81afffff11ff819fff81df8080ff
    ff04ff82016fffff04ffff04ff8204efffff10ff8206efffff12ffff11ff819f
    ff81df80ff17808080ffff04ff8205efff808080808080ffff04ffff04ff06ff
    ff04ffff0effff0169ffff0bffff0102ffff0bffff0101ff819f80ffff0bffff
    0101ff820def808080ff808080ffff04ffff04ffff0181d6ffff04ff04ffff04
    ff05ffff04ff81dfffff04ffff04ff05ff8080ff808080808080ff80808080ff
    ff01ff088080ff0180ffff04ffff01ff333eff018080
    "
);

pub const REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    96d818294183b3534a45592740b854b7521b76774a69d2e5fcd5e0e33af80e88
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
