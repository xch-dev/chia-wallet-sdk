use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE: [u8; 817] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff04ffff04ff819fffff04ffff11ff82015fff0280ff
    8201df8080ffff04ffff04ffff0155ffff04ff82017fff808080ffff04ffff02
    ff3dffff04ffff04ff15ff2d80ffff04ff0bffff02ff19ffff04ff11ffff04ff
    ff04ff81bfff82017f80ffff04ff820fffff820bff80808080808080ffff04ff
    ff04ffff0133ffff04ffff02ff15ffff04ff2dffff04ff0bffff04ffff0bffff
    0101ffff02ff19ffff04ff11ffff04ffff04ffff10ff81bfffff010180ff8201
    7f80ffff04ff820fffffff11ff820bffff02808080808080ff8080808080ffff
    04ff80ffff04ffff04ffff0bffff0101ff82017f80ff8080ff8080808080ffff
    04ffff02ff3dffff04ffff04ff15ff2d80ffff04ff17ffff02ff11ffff04ff82
    017fffff04ff8202ffff8205ff808080808080ffff04ffff04ffff0143ffff04
    ffff0112ffff04ffff0effff0177ffff0bffff0102ffff0bffff0101ff82017f
    80ffff0bffff0101ff8205ff808080ffff04ff8202ffff8080808080ffff04ff
    ff04ffff0181d6ffff04ffff0133ffff04ff8202ffffff04ff02ffff04ffff04
    ff8202ffff8080ff808080808080ff8080808080808080ffff04ffff13ffff12
    ff8202ffff1780ffff0182271080ff018080ffff04ffff04ffff04ffff01ff0b
    ffff0102ffff0bffff0101ff0280ffff0bffff0102ffff0bffff0101ff0580ff
    ff0bffff0101ff07808080ffff01ff0bffff0102ffff0bffff0101ff0980ffff
    02ff02ffff04ff0dff0780808080ffff04ffff01ff0bffff0102ffff0bffff01
    82010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ff
    ff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ff
    ff04ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ff
    ff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff01
    02ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bff
    ff018201018080ff0180ffff01ff04ffff0142ffff04ffff0112ffff04ff80ff
    ff04ffff02ff04ffff04ff06ffff04ff05ffff04ffff0bffff0101ff0780ff80
    80808080ff8080808080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    3bc68bef318e3d6a1c2a6002e9f6f56cba0fe2a81404adcec99055b467df05a0
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
    pub reward_slot_counter: u64,
    pub reward_slot_epoch_time: u64,
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
