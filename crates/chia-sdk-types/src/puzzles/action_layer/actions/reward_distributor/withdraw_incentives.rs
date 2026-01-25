use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE: [u8; 832] = hex!(
    "
    ff02ffff01ff04ffff04ff4fffff04ffff11ff81afffff02ffff03ffff09ff81
    bfffff05ffff14ffff12ff17ff8202ff80ffff01822710808080ffff0181bfff
    ff01ff088080ff018080ff81ef8080ffff04ffff04ff10ffff04ff5fff808080
    ffff04ffff02ff3effff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ff
    ff04ff5fffff04ff8207ffffff04ff8205ffff808080808080ff8080808080ff
    ff04ffff02ff16ffff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff
    04ff5fffff04ff8207ffffff04ffff11ff8205ffff81bf80ff808080808080ff
    ff04ffff0bffff0101ff5f80ff808080808080ffff04ffff02ff3effff04ff02
    ffff04ff0bffff04ffff02ff2effff04ff02ffff04ff5fffff04ff82017fffff
    04ff8202ffff808080808080ff8080808080ffff04ffff04ff14ffff04ffff01
    12ffff04ffff0effff0177ffff0bffff0102ffff0bffff0101ff5f80ffff0bff
    ff0101ff8202ff808080ffff04ff82017fff8080808080ffff04ffff04ffff01
    81d6ffff04ff18ffff04ff82017fffff04ff81bfffff04ffff04ff82017fff80
    80ff808080808080ff8080808080808080ffff04ffff01ffffff5533ff4342ff
    ff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce2
    3c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78
    f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759
    d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e72
    98a91ce119a63400ade7c5ffff04ff18ffff04ffff0bff5affff0bff12ffff0b
    ff12ff6aff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12ff6affff0b
    ffff0101ff0b8080ffff0bff12ff6aff4a808080ff4a808080ffff04ff80ffff
    04ffff04ff17ff8080ff8080808080ffff0bffff0102ffff0bffff0101ff0580
    ffff0bffff0102ffff0bffff0101ff0b80ffff0bffff0101ff17808080ff04ff
    1cffff04ffff0112ffff04ff80ffff04ffff0bff5affff0bff12ffff0bff12ff
    6aff0580ffff0bff12ffff0bff7affff0bff12ffff0bff12ff6affff0bffff01
    01ff0b8080ffff0bff12ff6aff4a808080ff4a808080ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    fe0bf3bdab042a51e848f813098e322be1e394f469032c3efa00a14b96bdfd6e
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorWithdrawIncentivesActionArgs {
    pub reward_slot_1st_curry_hash: Bytes32,
    pub commitment_slot_1st_curry_hash: Bytes32,
    pub withdrawal_share_bps: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorWithdrawIncentivesActionSolution {
    pub reward_slot_epoch_time: u64,
    pub withdrawal_share: u64,
    pub clawback_ph: Bytes32,
    pub committed_value: u64,
    pub reward_slot_total_rewards: u64,
    #[clvm(rest)]
    pub reward_slot_next_epoch_initialized: bool,
}

impl Mod for RewardDistributorWithdrawIncentivesActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE_HASH
    }
}
