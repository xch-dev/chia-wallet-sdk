use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE: [u8; 263] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff15ff8207efff8205ef80ffff15ff5fff80
    80ffff09ff7fffff05ffff14ffff12ff5fff0b80ffff0182271080808080ffff
    01ff04ffff04ff4fffff04ffff10ff81afffff11ff5fff7f8080ffff04ff8201
    6fffff04ffff04ff8204efffff10ff8206efffff12ffff11ff5fff7f80ff1780
    8080ff8203ef80808080ffff04ffff04ff06ffff04ffff0effff0169ffff0bff
    ff0102ffff0bffff0101ff5f80ffff0bffff0101ff8207ef808080ff808080ff
    ff04ffff04ffff0181d6ffff04ff04ffff04ff05ffff04ff7fffff04ffff04ff
    05ff8080ff808080808080ff80808080ffff01ff088080ff0180ffff04ffff01
    ff333eff018080
    "
);

pub const REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    8ca8bbf2f007208783cedbe2a552548b2aba68cf64b077d7f96cd824e29fc97f
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
