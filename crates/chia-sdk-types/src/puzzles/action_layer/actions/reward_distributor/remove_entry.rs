use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE: [u8; 671] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ff8202bfffff12ffff11ff8209dfff820bbf80
    ff820fbf8080ffff01ff04ffff04ff819fffff04ffff11ff82015fff8202bf80
    ffff04ffff11ff8202dfff820fbf80ff8203df808080ffff04ffff04ff1cffff
    04ffff0112ffff04ffff0effff0172ffff0bffff0102ffff0bffff0101ff8205
    bf80ffff0bffff0101ff820fbf808080ffff04ffff0bff56ffff0bff1affff0b
    ff1aff66ff0580ffff0bff1affff0bff76ffff0bff1affff0bff1aff66ff0b80
    ffff0bff1affff0bff76ffff0bff1affff0bff1aff66ff82013f80ffff0bff1a
    ff66ff46808080ff46808080ff46808080ff8080808080ffff04ffff04ff08ff
    ff04ffff10ff8213dfff2f80ff808080ffff04ffff02ff1effff04ff02ffff04
    ff17ffff04ffff0bffff0102ffff0bffff0101ff8205bf80ffff0bffff0102ff
    ff0bffff0101ff820bbf80ffff0bffff0101ff820fbf808080ff8080808080ff
    ff04ffff04ffff0181d6ffff04ff14ffff04ff8205bfffff04ff8202bfffff04
    ffff04ff8205bfff8080ff808080808080ff808080808080ffff01ff088080ff
    0180ffff04ffff01ffff55ff3343ffff4202ffffffa04bf5122f344554c53bde
    2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d1
    1a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210
    fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63
    fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ff12
    ffff04ffff0112ffff04ff80ffff04ffff0bff56ffff0bff1affff0bff1aff66
    ff0580ffff0bff1affff0bff76ffff0bff1affff0bff1aff66ffff0bffff0101
    ff0b8080ffff0bff1aff66ff46808080ff46808080ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    4cb611d7003037ead2cf96a08989f0f063db05dfc548fc68cee23fbcd6887bed
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorRemoveEntryActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub manager_singleton_struct_hash: Bytes32,
    pub entry_slot_1st_curry_hash: Bytes32,
    pub max_seconds_offset: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorRemoveEntryActionSolution {
    pub manager_singleton_inner_puzzle_hash: Bytes32,
    pub entry_payout_amount: u64,
    pub entry_payout_puzzle_hash: Bytes32,
    pub entry_initial_cumulative_payout: u64,
    #[clvm(rest)]
    pub entry_shares: u64,
}

impl Mod for RewardDistributorRemoveEntryActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE_HASH
    }
}
