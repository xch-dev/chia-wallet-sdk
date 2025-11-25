use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE: [u8; 138] = hex!(
    "
    ff02ffff03ffff22ff80ffff15ff81afffff0181ff80ffff15ff0bff81af80ffff20ffff15ff05ff4f808080ffff01ff04ffff04ff27ff3780ff8080ffff01ff08ffff12ffff11ff820277ff8202ef80ff8203ef80ffff10ffff12ff4fff0b80ff81af80ffff15ff81afffff0181ff80ffff15ff0bff81af80ffff20ffff15ff05ff4f80808080ff0180
    "
);

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    91b77f2799bafa052d376d5e81170a2381e5106b8d62b8c1e8f6a3644b78d91b
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorInitiatePayoutActionArgs {
    pub entry_slot_1st_curry_hash: Bytes32,
    pub payout_threshold: u64,
    pub precision: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorInitiatePayoutActionSolution {
    pub entry_payout_amount: u64,
    pub payout_rounding_error: u128,
    pub entry_payout_puzzle_hash: Bytes32,
    pub entry_initial_cumulative_payout: u128,
    #[clvm(rest)]
    pub entry_shares: u64,
}

impl Mod for RewardDistributorInitiatePayoutActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE_HASH
    }
}
