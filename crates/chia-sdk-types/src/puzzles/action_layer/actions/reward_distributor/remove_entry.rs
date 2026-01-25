use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{RewardDistributorEntryPayoutInfo, RewardDistributorEntrySlotValue},
    Mod,
};

pub const REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE: [u8; 737] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ffff12ffff11ff8213bfff82057f80ff
    82077f80ffff10ffff12ff8204ffff5f80ff8206ff8080ffff15ff8206ffffff
    0181ff80ffff15ff5fff8206ff8080ffff01ff04ffff04ff82013fffff04ffff
    11ff8202bfff8204ff80ffff04ffff11ff8205bfff82077f80ffff04ffff04ff
    8213bfffff10ff821bbfff8206ff8080ff820fbf80808080ffff04ffff04ff1c
    ffff04ffff0112ffff04ffff0effff0172ffff0bffff0102ffff0bffff0101ff
    82027f80ffff0bffff0101ff82077f808080ffff04ffff0bff56ffff0bff1aff
    ff0bff1aff66ff0580ffff0bff1affff0bff76ffff0bff1affff0bff1aff66ff
    0b80ffff0bff1affff0bff76ffff0bff1affff0bff1aff66ff8203ff80ffff0b
    ff1aff66ff46808080ff46808080ff46808080ff8080808080ffff04ffff04ff
    08ffff04ffff10ff8217bfff2f80ff808080ffff04ffff02ff1effff04ff02ff
    ff04ff17ffff04ffff0bffff0102ffff0bffff0101ff82027f80ffff0bffff01
    02ffff0bffff0101ff82057f80ffff0bffff0101ff82077f808080ff80808080
    80ffff04ffff04ffff0181d6ffff04ff14ffff04ff82027fffff04ff8204ffff
    ff04ffff04ff82027fff8080ff808080808080ff808080808080ffff01ff0880
    80ff0180ffff04ffff01ffff55ff3343ffff4202ffffffa04bf5122f344554c5
    3bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f326
    23d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fe
    e210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5
    dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04
    ff12ffff04ffff0112ffff04ff80ffff04ffff0bff56ffff0bff1affff0bff1a
    ff66ff0580ffff0bff1affff0bff76ffff0bff1affff0bff1aff66ffff0bffff
    0101ff0b8080ffff0bff1aff66ff46808080ff46808080ff8080808080ff0180
    80
    "
);

pub const REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    2ded48530bd00c27fa8dce6c697e5ff5332a634cb3eefd5a5e9713b434c3a3e6
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
    pub entry_payout_info: RewardDistributorEntryPayoutInfo,
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
