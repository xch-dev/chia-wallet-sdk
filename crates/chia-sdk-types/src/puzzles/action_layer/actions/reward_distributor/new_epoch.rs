use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE: [u8; 846] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ff8227bfff8237bf80ffff09ffff05ff
    ff14ffff12ff82177fff1780ffff018227108080ff821f7f80ffff21ffff22ff
    ff09ff82027fff8237bf80ffff09ff82177fff820b7f8080ffff22ffff15ff82
    37bfff82027f80ffff20ff82057f80ffff09ff82177fff8080808080ffff01ff
    04ffff04ff82013fffff04ffff11ff8202bfff821f7f80ffff04ff8205bfffff
    04ffff04ff8213bfffff10ff821bbfffff12ffff11ff82177fff821f7f80ff5f
    808080ffff04ffff04ff8237bfffff10ff8237bfff2f8080ff808080808080ff
    ff04ffff04ff14ffff04ffff0effff0165ffff0bffff0101ff8237bf8080ff80
    8080ffff04ffff04ffff0181d6ffff04ff08ffff04ff0bffff04ff821f7fffff
    04ffff04ff0bff8080ff808080808080ffff04ffff02ff1effff04ff02ffff04
    ff05ffff04ffff0bffff0102ffff0bffff0101ff82027f80ffff0bffff0102ff
    ff0bffff0101ff82057f80ffff0bffff0101ff820b7f808080ff8080808080ff
    ff04ffff02ff16ffff04ff02ffff04ff05ffff04ffff0bffff0102ffff0bffff
    0101ff82027f80ffff0bffff0102ffff0bffff0101ff82057f80ffff0bffff01
    01ff820b7f808080ffff04ffff0bffff0101ff82027f80ff808080808080ff80
    8080808080ffff01ff088080ff0180ffff04ffff01ffff33ff3e42ffff02ffff
    a04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c778545
    9aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718b
    a7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f680
    6923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce1
    19a63400ade7c5ffff04ff08ffff04ffff0bff5affff0bff12ffff0bff12ff6a
    ff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12ff6affff0bffff0101
    ff0b8080ffff0bff12ff6aff4a808080ff4a808080ffff04ff80ffff04ffff04
    ff17ff8080ff8080808080ff04ff1cffff04ffff0112ffff04ff80ffff04ffff
    0bff5affff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff
    12ffff0bff12ff6affff0bffff0101ff0b8080ffff0bff12ff6aff4a808080ff
    4a808080ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    22e3603c648fdd6bada6e92468af996d809f60cf4b2bb2bcb3bd082b36daeeb7
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
