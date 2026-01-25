use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{RewardDistributorEntryPayoutInfo, RewardDistributorEntrySlotValue},
    Mod,
};

pub const REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE: [u8; 963] = hex!(
    "
    ff02ffff01ff02ff16ffff04ff02ffff04ff03ffff04ffff02ff2fffff04ff81
    9fffff04ff82013fff8201ff808080ff8080808080ffff04ffff01ffffff55ff
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
    8080ffff02ffff03ffff22ffff09ffff12ff13ffff11ff8213bdff82057d8080
    ffff10ffff12ff8204fdff2d80ff8206fd8080ffff15ff8206fdffff0181ff80
    ffff15ff2dff8206fd80ffff20ffff15ff13ff82077d808080ffff01ff04ffff
    04ff82013dffff04ffff11ff8202bdff8204fd80ffff04ffff11ff8205bdff13
    80ffff04ffff04ff8213bdffff10ff821bbdff8206fd8080ff820fbd80808080
    ffff04ffff04ff10ffff04ffff10ff8217bdff1580ff808080ffff04ffff02ff
    3effff04ff02ffff04ff09ffff04ffff02ff2effff04ff02ffff04ff82017dff
    80808080ff8080808080ffff04ffff04ffff0181d6ffff04ff28ffff04ff8202
    7dffff04ff8204fdffff04ffff04ff82027dff8080ff808080808080ffff02ff
    ff03ffff09ff13ff82077d80ffff011bffff01ff04ffff02ff12ffff04ff02ff
    ff04ff09ffff04ffff02ff2effff04ff02ffff04ffff04ff82027dffff04ff82
    057dffff11ff82077dff13808080ff80808080ffff04ff82027dff8080808080
    80ff1b8080ff018080808080ffff01ff088080ff0180ffff02ffff03ffff07ff
    0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ff
    ff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff0580
    80ff0180ff04ff38ffff04ffff0112ffff04ff80ffff04ffff02ff2affff04ff
    02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ff8080808080ff
    018080
    "
);

pub const REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    4fcd686d7238ab740b534bd3260806b071f711544a39c25cad4370a8507e4ce3
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
