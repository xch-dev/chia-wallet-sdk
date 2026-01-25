use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::RewardDistributorEntrySlotValue, Mod};

pub const REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE: [u8; 743] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ffff12ffff11ff8213bfff82057f80ff
    82077f80ffff10ffff12ff8205ffff5f80ff8202ff8080ffff15ff8202ffffff
    0181ff80ffff15ff5fff8202ff8080ffff01ff04ffff04ff82013fffff04ffff
    11ff8202bfff8205ff80ffff04ffff11ff8205bfff82077f80ffff04ffff04ff
    8213bfffff10ff821bbfff8202ff8080ffff04ff8217bfff808080808080ffff
    04ffff04ff1cffff04ffff0112ffff04ffff0effff0172ffff0bffff0102ffff
    0bffff0101ff82027f80ffff0bffff0101ff82077f808080ffff04ffff0bff56
    ffff0bff1affff0bff1aff66ff0580ffff0bff1affff0bff76ffff0bff1affff
    0bff1aff66ff0b80ffff0bff1affff0bff76ffff0bff1affff0bff1aff66ff82
    07ff80ffff0bff1aff66ff46808080ff46808080ff46808080ff8080808080ff
    ff04ffff04ff08ffff04ffff10ff8227bfff2f80ff808080ffff04ffff02ff1e
    ffff04ff02ffff04ff17ffff04ffff0bffff0102ffff0bffff0101ff82027f80
    ffff0bffff0102ffff0bffff0101ff82057f80ffff0bffff0101ff82077f8080
    80ff8080808080ffff04ffff04ffff0181d6ffff04ff14ffff04ff82027fffff
    04ff8205ffffff04ffff04ff82027fff8080ff808080808080ff808080808080
    ffff01ff088080ff0180ffff04ffff01ffff55ff3343ffff4202ffffffa04bf5
    122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09d
    cf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ff
    a102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f6
    3222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a634
    00ade7c5ff04ff12ffff04ffff0112ffff04ff80ffff04ffff0bff56ffff0bff
    1affff0bff1aff66ff0580ffff0bff1affff0bff76ffff0bff1affff0bff1aff
    66ffff0bffff0101ff0b8080ffff0bff1aff66ff46808080ff46808080ff8080
    808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    d071e2ab9406bbc4d4369868138b403142591bb0a2ed0c24395ff0961545923b
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorRemoveEntryActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub manager_singleton_struct_hash: Bytes32,
    pub entry_slot_1st_curry_hash: Bytes32,
    pub max_seconds_offset: u64,
    pub precision: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorRemoveEntryActionSolution {
    pub entry_slot: RewardDistributorEntrySlotValue,
    pub payout_rounding_error: u128,
    pub entry_payout_amount: u64,
    #[clvm(rest)]
    pub manager_singleton_inner_puzzle_hash: Bytes32,
}

impl Mod for RewardDistributorRemoveEntryActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE_HASH
    }
}
