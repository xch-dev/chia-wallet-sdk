use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE: [u8; 789] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff02ffff03ffff22ffff09ff822f7fff823f7f80ffff
    21ffff22ffff09ff8205ffff823f7f80ffff09ff821fffff8217ff8080ffff22
    ffff22ffff15ff823f7fff8205ff80ffff20ff820bff8080ffff20ff821fff80
    808080ffff01ff02ffff01ff04ffff04ff8204ffffff04ffff11ff820affff05
    80ffff04ff8216ffffff04ffff04ff824effffff10ff826effffff12ffff11ff
    823fffff0580ff82017f808080ffff04ff827effffff10ff827effff81bf8080
    80808080ffff04ffff04ffff0151ffff04ff827effff808080ffff04ffff04ff
    ff013effff04ffff0effff0165ffff0bffff0101ff827eff8080ff808080ffff
    04ffff04ffff0181d6ffff04ffff0133ffff04ff2fffff04ff05ffff04ffff04
    ff2fff8080ff808080808080ffff04ffff04ffff0142ffff04ffff0112ffff04
    ff80ffff04ffff02ff13ffff04ff1bffff04ff17ffff04ffff0bffff0101ffff
    0bffff0102ffff0bffff0101ff8205ff80ff028080ff8080808080ff80808080
    80ffff04ffff04ffff0133ffff04ffff02ff13ffff04ff1bffff04ff17ffff04
    ffff0bffff0101ffff0bffff0102ffff0bffff0101ffff10ff8205ffffff0101
    8080ff028080ff8080808080ffff04ff80ffff04ffff04ffff0bffff0101ff82
    0bff80ff8080ff8080808080ff80808080808080ffff04ffff0bffff0102ffff
    0bffff0101ff8205ff80ffff0bffff0102ffff0bffff0101ff820bff80ffff0b
    ffff0101ff8217ff808080ff018080ffff01ff088080ff0180ffff04ffff13ff
    ff12ff820fffff1780ffff0182271080ff018080ffff04ffff04ffff01ff0bff
    ff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff0102ffff0bffff
    0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0b
    ffff010180808080ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff
    0182010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580
    ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080
    ffff01ff0bffff018201018080ff018080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    1b2c758b5a4da560bf177ab23b9500dcdb302bc7724b39c82b641c214d13332f
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
    #[clvm(rest)]
    pub epoch_total_rewards: u64,
}

impl Mod for RewardDistributorNewEpochActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE_HASH
    }
}
