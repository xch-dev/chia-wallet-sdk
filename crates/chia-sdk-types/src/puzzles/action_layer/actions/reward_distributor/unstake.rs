use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{RewardDistributorEntryPayoutInfo, RewardDistributorEntrySlotValue},
};

pub const REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE: [u8; 780] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff02ffff03ffff22ffff22ffff22ffff09ffff12ff04
    ffff11ff8213bfff820b7f8080ffff10ffff12ff8204ffff2f80ff8206ff8080
    ffff15ff8206ffffff0181ff8080ffff15ff2fff8206ff8080ffff21ffff15ff
    820f7fff0480ffff09ff04ff820f7f808080ffff01ff04ffff04ff82013fffff
    04ffff11ff8202bfff8204ff80ffff04ffff11ff8205bfff0480ffff04ffff04
    ff8213bfffff10ff821bbfff8206ff8080ff820fbf80808080ffff04ffff04ff
    ff0155ffff04ffff10ff8217bfff1780ff808080ffff04ffff04ffff0142ffff
    04ffff0112ffff04ff80ffff04ffff02ff15ffff04ff1dffff04ff0bffff04ff
    ff0bffff0101ffff02ff09ffff04ff09ff82017f808080ff8080808080ff8080
    808080ffff04ffff04ffff0181d6ffff04ffff0133ffff04ff82057fffff04ff
    8204ffffff04ffff04ff82057fff8080ff808080808080ffff04ffff04ffff01
    33ffff04ffff02ff15ffff04ff1dffff04ff0bffff04ffff0bffff0101ffff02
    ff09ffff04ff09ffff04ffff10ff82027fffff010180ffff04ff82057fffff04
    ff820b7fffff11ff820f7fff0480808080808080ff8080808080ffff04ff80ff
    ff04ffff04ff82057fff8080ff8080808080ff068080808080ffff01ff088080
    ff0180ffff04ffff02ff2fffff04ff819fffff04ff8202bfff8201ff808080ff
    018080ffff04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102
    ffff02ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01
    ff0bffff0101ff038080ff0180ffff04ffff01ff0bffff0102ffff0bffff0182
    010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff
    0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff
    01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff
    0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02
    ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bffff018201
    018080ff01808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    32b509018dab93dd22d6f7737b78b3059fcf5f1adeb561f81e5b65d223fcbf8d
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorUnstakeActionArgs<UP> {
    pub entry_slot_1st_curry_hash: Bytes32,
    pub max_second_offset: u64,
    pub precision: u64,
    pub unlock_puzzle: UP,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorUnstakeActionSolution<UPS> {
    pub entry_slot: RewardDistributorEntrySlotValue,
    pub entry_payout_info: RewardDistributorEntryPayoutInfo,
    #[clvm(rest)]
    pub unlock_puzzle_solution: UPS,
}

impl<UP> Mod for RewardDistributorUnstakeActionArgs<UP> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE_HASH
    }
}
