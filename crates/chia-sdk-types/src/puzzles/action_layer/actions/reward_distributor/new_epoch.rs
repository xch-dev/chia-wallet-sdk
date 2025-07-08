use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE: [u8; 839] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ff8213dfff821bdf80ffff09ffff05ff
    ff14ffff12ff820bbfff1780ffff018227108080ff820fbf80ffff21ffff22ff
    ff09ff82013fff821bdf80ffff09ff820bbfff8205bf8080ffff22ffff15ff82
    1bdfff82013f80ffff20ff8202bf80ffff09ff820bbfff8080808080ffff01ff
    04ffff04ff819fffff04ffff11ff82015fff820fbf80ffff04ff8202dfffff04
    ffff04ff8209dfffff10ff820ddfffff11ff820bbfff820fbf808080ffff04ff
    ff04ff821bdfffff10ff821bdfff2f8080ff808080808080ffff04ffff04ff14
    ffff04ffff0effff0165ffff0bffff0101ff821bdf8080ff808080ffff04ffff
    04ffff0181d6ffff04ff08ffff04ff0bffff04ff820fbfffff04ffff04ff0bff
    8080ff808080808080ffff04ffff02ff1effff04ff02ffff04ff05ffff04ffff
    0bffff0102ffff0bffff0101ff82013f80ffff0bffff0102ffff0bffff0101ff
    8202bf80ffff0bffff0101ff8205bf808080ff8080808080ffff04ffff02ff16
    ffff04ff02ffff04ff05ffff04ffff0bffff0102ffff0bffff0101ff82013f80
    ffff0bffff0102ffff0bffff0101ff8202bf80ffff0bffff0101ff8205bf8080
    80ffff04ffff0bffff0101ff82013f80ff808080808080ff808080808080ffff
    01ff088080ff0180ffff04ffff01ffff33ff3e42ffff02ffffa04bf5122f3445
    54c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184
    f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a128
    71fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102
    a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5
    ffff04ff08ffff04ffff0bff5affff0bff12ffff0bff12ff6aff0580ffff0bff
    12ffff0bff7affff0bff12ffff0bff12ff6affff0bffff0101ff0b8080ffff0b
    ff12ff6aff4a808080ff4a808080ffff04ff80ffff04ffff04ff17ff8080ff80
    80808080ff04ff1cffff04ffff0112ffff04ff80ffff04ffff0bff5affff0bff
    12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12ff
    6affff0bffff0101ff0b8080ffff0bff12ff6aff4a808080ff4a808080ff8080
    808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    ac01b2b3c3c137fa08662cf51e7eb28a238de85dbb8759050f39ef3dc461bfb9
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorNewEpochActionArgs {
    pub reward_slot_1st_curry_hash: Bytes32,
    pub fee_payout_puzzle_hash: Bytes32,
    pub fee_bps: u64,
    pub epoch_seconds: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorNewEpochActionSolution {
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
