use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE: [u8; 826] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff09ff82017fffff13ffff12ff8205ffff1780ffff
    018227108080ffff01ff04ffff04ff4fffff04ffff11ff81afff82017f80ff81
    ef8080ffff04ffff04ffff0155ffff04ff81bfff808080ffff04ffff02ff1eff
    ff04ffff04ff0aff1680ffff04ff05ffff02ff0cffff04ff08ffff04ffff04ff
    5fff81bf80ffff04ff820fffff820bff80808080808080ffff04ffff04ffff01
    33ffff04ffff02ff0affff04ff16ffff04ff05ffff04ffff0bffff0101ffff02
    ff0cffff04ff08ffff04ffff04ffff10ff5fffff010180ff81bf80ffff04ff82
    0fffffff11ff820bffff82017f808080808080ff8080808080ffff04ff80ffff
    04ffff04ffff0bffff0101ff81bf80ff8080ff8080808080ffff04ffff02ff1e
    ffff04ffff04ff0aff1680ffff04ff0bffff02ff08ffff04ff81bfffff04ff82
    02ffff8205ff808080808080ffff04ffff04ffff0143ffff04ffff0112ffff04
    ffff0effff0177ffff0bffff0102ffff0bffff0101ff81bf80ffff0bffff0101
    ff8205ff808080ffff04ff8202ffff8080808080ffff04ffff04ffff0181d6ff
    ff04ffff0133ffff04ff8202ffffff04ff82017fffff04ffff04ff8202ffff80
    80ff808080808080ff8080808080808080ffff01ff088080ff0180ffff04ffff
    04ffff04ffff01ff0bffff0102ffff0bffff0101ff0280ffff0bffff0102ffff
    0bffff0101ff0580ffff0bffff0101ff07808080ffff01ff0bffff0102ffff0b
    ffff0101ff0980ffff02ff02ffff04ff0dff0780808080ffff04ffff01ff0bff
    ff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff0102ffff0bffff
    0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0b
    ffff010180808080ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102ffff
    0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180
    ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180
    808080ffff01ff0bffff018201018080ff0180ffff01ff04ffff0142ffff04ff
    ff0112ffff04ff80ffff04ffff02ff04ffff04ff06ffff04ff05ffff04ffff0b
    ffff0101ff0780ff8080808080ff8080808080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    4c0b2d8553346910af587b26d3ddcf160191ddd1c0a1773092a3c66ec1f478eb
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
