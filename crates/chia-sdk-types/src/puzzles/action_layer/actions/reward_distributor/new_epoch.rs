use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE: [u8; 771] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff22ffff09ff8217bfff821fbf80ffff09ff
    821fffffff13ffff12ff8217ffff1780ffff01822710808080ffff21ffff22ff
    ff09ff8202ffff821fbf80ffff09ff8217ffff820bff8080ffff22ffff22ffff
    15ff821fbfff8202ff80ffff20ff8205ff8080ffff20ff8217ff80808080ffff
    01ff02ffff01ff04ffff04ff82027fffff04ffff11ff82057fff823fff80ffff
    04ff820b7fffff04ffff04ff82277fffff10ff82377fffff12ffff11ff822fff
    ff823fff80ff81bf808080ffff04ff823f7fffff10ff823f7fff5f8080808080
    80ffff04ffff04ffff013effff04ffff0effff0165ffff0bffff0101ff823f7f
    8080ff808080ffff04ffff04ffff0181d6ffff04ffff0133ffff04ff17ffff04
    ff823fffffff04ffff04ff17ff8080ff808080808080ffff04ffff04ffff0142
    ffff04ffff0112ffff04ff80ffff04ffff02ff09ffff04ff0dffff04ff0bffff
    04ffff0bffff0101ffff0bffff0102ffff0bffff0101ff8202ff80ff028080ff
    8080808080ff8080808080ffff04ffff04ffff0133ffff04ffff02ff09ffff04
    ff0dffff04ff0bffff04ffff0bffff0101ffff0bffff0102ffff0bffff0101ff
    ff10ff8202ffffff01018080ff028080ff8080808080ffff04ff80ffff04ffff
    04ffff0bffff0101ff8205ff80ff8080ff8080808080ff808080808080ffff04
    ffff0bffff0102ffff0bffff0101ff8202ff80ffff0bffff0102ffff0bffff01
    01ff8205ff80ffff0bffff0101ff820bff808080ff018080ffff01ff088080ff
    0180ffff04ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff
    0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02
    ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff02ffff03ff03
    ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff01
    02ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff
    078080ffff0bffff010180808080ffff01ff0bffff018201018080ff018080ff
    018080
    "
);

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    2b18e5c69af11362bd7fe29c6ebbce78e51042a126de1a38148f9dcd7ad539b5
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorNewEpochActionArgs {
    pub reward_slot_1st_curry_hash: Bytes32,
    pub fee_payout_puzzle_hash: Bytes32,
    pub fee_bps: u64,
    pub epoch_seconds: u64,
    pub precision: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorNewEpochActionSolution {
    pub slot_counter: u64,
    pub slot_epoch_time: u64,
    pub slot_next_epoch_initialized: bool,
    pub slot_total_rewards: u64,
    pub epoch_total_rewards: u64,
    #[clvm(rest)]
    pub fee: u64,
}

impl Mod for RewardDistributorNewEpochActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE_HASH
    }
}
