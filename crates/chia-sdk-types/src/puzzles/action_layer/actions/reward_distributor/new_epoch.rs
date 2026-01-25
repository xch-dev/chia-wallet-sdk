use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE: [u8; 840] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ff8217bfff821fbf80ffff09ffff05ff
    ff14ffff12ff820bffff1780ffff018227108080ff820fff80ffff21ffff22ff
    ff09ff82017fff821fbf80ffff09ff820bffff8205ff8080ffff22ffff15ff82
    1fbfff82017f80ffff20ff8202ff80ffff09ff820bffff8080808080ffff01ff
    04ffff04ff82013fffff04ffff11ff8202bfff820fff80ffff04ff8205bfffff
    04ffff04ff8213bfffff10ff821bbfffff12ffff11ff820bffff820fff80ff5f
    808080ffff04ff821fbfffff10ff821fbfff2f808080808080ffff04ffff04ff
    14ffff04ffff0effff0165ffff0bffff0101ff821fbf8080ff808080ffff04ff
    ff04ffff0181d6ffff04ff08ffff04ff0bffff04ff820fffffff04ffff04ff0b
    ff8080ff808080808080ffff04ffff02ff1effff04ff02ffff04ff05ffff04ff
    ff0bffff0102ffff0bffff0101ff82017f80ffff0bffff0102ffff0bffff0101
    ff8202ff80ffff0bffff0101ff8205ff808080ff8080808080ffff04ffff02ff
    16ffff04ff02ffff04ff05ffff04ffff0bffff0102ffff0bffff0101ff82017f
    80ffff0bffff0102ffff0bffff0101ff8202ff80ffff0bffff0101ff8205ff80
    8080ffff04ffff0bffff0101ff82017f80ff808080808080ff808080808080ff
    ff01ff088080ff0180ffff04ffff01ffff33ff3e42ffff02ffffa04bf5122f34
    4554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a1
    84f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a1
    2871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a1
    02a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7
    c5ffff04ff08ffff04ffff0bff5affff0bff12ffff0bff12ff6aff0580ffff0b
    ff12ffff0bff7affff0bff12ffff0bff12ff6affff0bffff0101ff0b8080ffff
    0bff12ff6aff4a808080ff4a808080ffff04ff80ffff04ffff04ff17ff8080ff
    8080808080ff04ff1cffff04ffff0112ffff04ff80ffff04ffff0bff5affff0b
    ff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12
    ff6affff0bffff0101ff0b8080ffff0bff12ff6aff4a808080ff4a808080ff80
    80808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    4468ce0fff308d5048a94d1da0ca3d7a95e033f6f816bc0b80642f529cc8005b
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
