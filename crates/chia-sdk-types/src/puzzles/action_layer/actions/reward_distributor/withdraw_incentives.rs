use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE: [u8; 842] = hex!(
    "
    ff02ffff01ff04ffff04ff4fffff04ffff11ff81afffff02ffff03ffff09ff82
    0fdfffff05ffff14ffff12ff17ff820bdf80ffff01822710808080ffff01820f
    dfffff01ff088080ff018080ff81ef8080ffff04ffff04ff10ffff04ff819fff
    808080ffff04ffff02ff3effff04ff02ffff04ff05ffff04ffff02ff2effff04
    ff02ffff04ff819fffff04ff82015fffff04ff8202dfff808080808080ff8080
    808080ffff04ffff02ff16ffff04ff02ffff04ff05ffff04ffff02ff2effff04
    ff02ffff04ff819fffff04ff82015fffff04ffff11ff8202dfff820fdf80ff80
    8080808080ffff04ffff0bffff0101ff819f80ff808080808080ffff04ffff02
    ff3effff04ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ff819fff
    ff04ff8205dfffff04ff820bdfff808080808080ff8080808080ffff04ffff04
    ff14ffff04ffff0112ffff04ffff0effff0177ffff0bffff0102ffff0bffff01
    01ff819f80ffff0bffff0101ff820bdf808080ffff04ff8205dfff8080808080
    ffff04ffff04ffff0181d6ffff04ff18ffff04ff8205dfffff04ff820fdfffff
    04ffff04ff8205dfff8080ff808080808080ff8080808080808080ffff04ffff
    01ffffff5533ff4342ffff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1
    600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5
    709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea1
    94581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e
    8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff04ff18ffff04ffff0b
    ff5affff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff12
    ffff0bff12ff6affff0bffff0101ff0b8080ffff0bff12ff6aff4a808080ff4a
    808080ffff04ff80ffff04ffff04ff17ff8080ff8080808080ffff0bffff0102
    ffff0bffff0101ff0580ffff0bffff0102ffff0bffff0101ff0b80ffff0bffff
    0101ff17808080ff04ff1cffff04ffff0112ffff04ff80ffff04ffff0bff5aff
    ff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff0bff12ffff0b
    ff12ff6affff0bffff0101ff0b8080ffff0bff12ff6aff4a808080ff4a808080
    ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    934c0da44da945cf719eefa22da82258a575eb3798a4ed77b9239130bf38397b
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
    pub reward_slot_next_epoch_initialized: bool,
    pub reward_slot_total_rewards: u64,
    pub clawback_ph: Bytes32,
    pub committed_value: u64,
    #[clvm(rest)]
    pub withdrawal_share: u64,
}

impl Mod for RewardDistributorWithdrawIncentivesActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE_HASH
    }
}
