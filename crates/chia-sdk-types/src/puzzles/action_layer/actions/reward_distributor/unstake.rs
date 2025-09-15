use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::RewardDistributorEntrySlotValue, Mod};

pub const REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE: [u8; 969] = hex!(
    "
    ff02ffff01ff02ff16ffff04ff02ffff04ff03ffff04ffff02ff2fffff04ff81
    9fffff04ff821fbfff82013f808080ff8080808080ffff04ffff01ffffff55ff
    3342ff02ffff02ffff03ff05ffff01ff0bff81fcffff02ff3affff04ff02ffff
    04ff09ffff04ffff02ff2cffff04ff02ffff04ff0dff80808080ff8080808080
    80ffff0181dc80ff0180ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600a
    d631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b
    083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea19458
    1cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c
    1e1879b7152a6e7298a91ce119a63400ade7c5ffffff04ff28ffff04ffff02ff
    2affff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ffff
    04ff80ffff04ffff04ff17ff8080ff8080808080ffff0bff81bcffff02ff3aff
    ff04ff02ffff04ff05ffff04ffff02ff2cffff04ff02ffff04ff07ff80808080
    ff808080808080ff0bff14ffff0bff14ff81dcff0580ffff0bff14ff0bff819c
    8080ffff02ffff03ffff22ffff09ffff12ff13ffff11ff8213bdff82577d8080
    ffff10ffff12ff82057dff2d80ff820b7d8080ffff15ff820b7dffff0181ff80
    ffff15ff2dff820b7d80ffff20ffff15ff13ff82277d808080ffff01ff04ffff
    04ff82013dffff04ffff11ff8202bdff82057d80ffff04ffff11ff8205bdff13
    80ffff04ffff04ff8213bdffff10ff821bbdff820b7d8080ffff04ff8217bdff
    808080808080ffff04ffff04ff10ffff04ffff10ff8227bdff1580ff808080ff
    ff04ffff02ff3effff04ff02ffff04ff09ffff04ffff02ff2effff04ff02ffff
    04ff82177dff80808080ff8080808080ffff04ffff04ffff0181d6ffff04ff28
    ffff04ff82777dffff04ff82057dffff04ffff04ff82777dff8080ff80808080
    8080ffff02ffff03ffff09ff13ff82277d80ffff011bffff01ff04ffff02ff12
    ffff04ff02ffff04ff09ffff04ffff02ff2effff04ff02ffff04ffff04ff8277
    7dffff04ff82577dffff11ff82277dff13808080ff80808080ffff04ff82777d
    ff808080808080ff1b8080ff018080808080ffff01ff088080ff0180ffff02ff
    ff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09
    ff80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bff
    ff0101ff058080ff0180ff04ff38ffff04ffff0112ffff04ff80ffff04ffff02
    ff2affff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ff
    8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    3356550fe1908fe89d5a6f75f143f80e484c8b05df85dccfafbe15b1283acede
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
    pub unlock_puzzle_solution: UPS,
    pub entry_payout_amount: u64,
    pub payout_rounding_error: u128,
    #[clvm(rest)]
    pub entry_slot: RewardDistributorEntrySlotValue,
}

impl<UP> Mod for RewardDistributorUnstakeActionArgs<UP> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE_HASH
    }
}
