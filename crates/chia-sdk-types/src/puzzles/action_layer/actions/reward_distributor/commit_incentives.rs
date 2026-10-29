use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE: [u8; 1089] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff20ffff15ff8207efff8205ff8080ffff15
    ff820fffff808080ffff01ff02ffff01ff04ffff04ff819fffff04ffff10ff82
    015fff821fff80ff8201df8080ffff04ffff04ffff013effff04ffff0effff01
    63ff0280ff808080ffff04ffff02ff19ffff04ff1dffff04ff17ffff04ff02ff
    8217ff80808080ffff04ffff04ffff0142ffff04ffff0112ffff04ff80ffff04
    ffff02ff2dffff04ff3dffff04ff0bffff04ffff0bffff0101ffff02ff11ffff
    04ff35ffff04ffff04ff81bfff82017f80ffff04ff8202ffff8205ff80808080
    80ff8080808080ff8080808080ffff02ffff03ffff09ff820bffff82017f80ff
    ff01ff04ffff02ff19ffff04ff1dffff04ff0bffff04ffff02ff11ffff04ff35
    ffff04ffff04ffff10ff81bfffff010180ff82017f80ffff04ff8202ffffff10
    ff8205ffff821fff8080808080ffff0bffff0101ff82017f8080808080ff8080
    ffff01ff02ffff03ff8202ffffff01ff0880ffff01ff04ffff02ff19ffff04ff
    1dffff04ff0bffff04ffff02ff11ffff04ff35ffff04ffff04ffff10ff81bfff
    ff010180ff82017f80ffff04ffff0101ff8205ff80808080ffff0bffff0101ff
    82017f8080808080ffff04ffff02ff19ffff04ff1dffff04ff0bffff04ffff02
    ff11ffff04ff35ffff04ffff04ff80ff820bff80ffff04ff80ff821fff808080
    80ffff0bffff0101ff820bff8080808080ffff02ff25ffff04ff05ffff04ffff
    10ff82017fff2f80ffff04ff0bffff04ff2fff820bff8080808080808080ff01
    8080ff018080808080ffff04ffff02ff1aff8203ff80ff018080ffff01ff0880
    80ff0180ffff04ffff04ffff04ffff01ff0bffff0102ffff0bffff0101ff0980
    ffff02ff02ffff04ff0dff07808080ffff01ff04ffff0133ffff04ffff02ff04
    ffff04ff06ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ffff04
    ff80ffff04ffff04ff0fff8080ff808080808080ffff04ffff04ffff01ff02ff
    ff03ffff09ff05ff1f80ffff0180ffff01ff04ffff02ff0cffff04ff0effff04
    ff0bffff04ffff02ff08ffff04ff1affff04ffff04ff80ff0580ffff01ff01
    80808080ffff0bffff0101ff058080808080ffff02ff12ffff04ff02ff
    ff04ffff10ff05ff1780ff078080808080ff0180ffff01ff0bffff0102ffff0b
    ffff0101ff0280ffff0bffff0102ffff0bffff0101ff0580ffff0bffff0101ff
    0780808080ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff
    0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02
    ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff02ffff03ff03
    ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff01
    02ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff
    078080ffff0bffff010180808080ffff01ff0bffff018201018080ff01808080
    80ff018080
    "
);

pub const REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    dd092298c7331f56f00b34cb68425a4f34bac28729a60daff7384a517087d3ec
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorCommitIncentivesActionArgs {
    pub reward_slot_1st_curry_hash: Bytes32,
    pub commitment_slot_1st_curry_hash: Bytes32,
    pub epoch_seconds: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorCommitIncentivesActionSolution {
    pub slot_counter: u64,
    pub slot_epoch_time: u64,
    pub slot_next_epoch_initialized: bool,
    pub slot_total_rewards: u64,
    pub epoch_start: u64,
    pub clawback_ph: Bytes32,
    #[clvm(rest)]
    pub rewards_to_add: u64,
}

impl Mod for RewardDistributorCommitIncentivesActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE_HASH
    }
}
