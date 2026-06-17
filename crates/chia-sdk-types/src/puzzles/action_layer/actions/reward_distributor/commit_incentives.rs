use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE: [u8; 1105] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff21ffff15ff8205ffff8207ef80ffff09ff
    8205ffff8207ef8080ffff15ff820fffff808080ffff01ff02ffff01ff04ffff
    04ff819fffff04ffff10ff82015fff821fff80ff8201df8080ffff04ffff04ff
    ff013effff04ffff0effff0163ff0280ff808080ffff04ffff02ff19ffff04ff
    1dffff04ff17ffff04ff02ff8217ff80808080ffff04ffff04ffff0142ffff04
    ffff0112ffff04ff80ffff04ffff02ff2dffff04ff3dffff04ff0bffff04ffff
    0bffff0101ffff02ff11ffff04ff35ffff04ffff04ff81bfff82017f80ffff04
    ff8202ffff8205ff8080808080ff8080808080ff8080808080ffff02ffff03ff
    ff09ff820bffff82017f80ffff01ff04ffff02ff19ffff04ff1dffff04ff0bff
    ff04ffff02ff11ffff04ff35ffff04ffff04ffff10ff81bfffff010180ff8201
    7f80ffff04ff8202ffffff10ff8205ffff821fff8080808080ffff0bffff0101
    ff82017f8080808080ff8080ffff01ff02ffff03ff8202ffffff01ff0880ffff
    01ff04ffff02ff19ffff04ff1dffff04ff0bffff04ffff02ff11ffff04ff35ff
    ff04ffff04ffff10ff81bfffff010180ff82017f80ffff04ffff0101ff8205ff
    80808080ffff0bffff0101ff82017f8080808080ffff04ffff02ff19ffff04ff
    1dffff04ff0bffff04ffff02ff11ffff04ff35ffff04ffff04ff80ff820bff80
    ffff04ff80ff821fff80808080ffff0bffff0101ff820bff8080808080ffff02
    ff25ffff04ff05ffff04ffff10ff82017fff2f80ffff04ff0bffff04ff2fff82
    0bff8080808080808080ff018080ff018080808080ffff04ffff02ff1aff8203
    ff80ff018080ffff01ff088080ff0180ffff04ffff04ffff04ffff01ff0bffff
    0102ffff0bffff0101ff0980ffff02ff02ffff04ff0dff07808080ffff01ff04
    ffff0133ffff04ffff02ff04ffff04ff06ffff04ff05ffff04ffff0bffff0101
    ff0b80ff8080808080ffff04ff80ffff04ffff04ff0fff8080ff808080808080
    ffff04ffff04ffff01ff02ffff03ffff09ff05ff1f80ffff0180ffff01ff04ff
    ff02ff0cffff04ff0effff04ff0bffff04ffff02ff08ffff04ff1affff04ffff
    04ff80ff0580ffff04ffff0101ff8080808080ffff0bffff0101ff0580808080
    80ffff02ff12ffff04ff02ffff04ffff10ff05ff1780ff078080808080ff0180
    ffff01ff0bffff0102ffff0bffff0101ff0280ffff0bffff0102ffff0bffff01
    01ff0580ffff0bffff0101ff0780808080ffff04ffff01ff0bffff0102ffff0b
    ffff0182010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff
    0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff01018080
    8080ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ff
    ff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff01
    02ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bff
    ff018201018080ff0180808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    c9abc3edba829c1af181eb143634a5ef68ab29cf944a8032c6781183b8925c85
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
