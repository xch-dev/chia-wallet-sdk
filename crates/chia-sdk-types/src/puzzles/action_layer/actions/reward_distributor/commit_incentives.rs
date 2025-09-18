use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE: [u8; 1209] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff20ffff15ff820defff8205df8080ffff15
    ff820fdfff808080ffff01ff04ffff04ff4fffff04ffff10ff81afff820fdf80
    ffff04ff82016fffff04ff8202efffff04ff8205efff808080808080ffff02ff
    12ffff04ff02ffff04ff0bffff04ffff0bffff0102ffff0bffff0101ff8205df
    80ffff0bffff0102ffff0bffff0101ff820bdf80ffff0bffff0101ff820fdf80
    8080ffff04ff820bdfffff04ffff04ffff02ff3effff04ff02ffff04ff05ffff
    04ffff02ff16ffff04ff02ffff04ff819fffff04ff82015fffff04ff8202dfff
    808080808080ff8080808080ffff02ffff03ffff09ff8205dfff819f80ffff01
    ff04ffff02ff1affff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff
    04ff819fffff04ff82015fffff04ffff10ff8202dfff820fdf80ff8080808080
    80ffff04ffff0bffff0101ff819f80ff808080808080ff8080ffff01ff02ffff
    03ff82015fffff01ff0880ffff01ff04ffff02ff1affff04ff02ffff04ff05ff
    ff04ffff02ff16ffff04ff02ffff04ff819fffff04ffff0101ffff04ff8202df
    ff808080808080ffff04ffff0bffff0101ff819f80ff808080808080ffff04ff
    ff02ff1affff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff82
    05dfffff04ff80ffff04ff820fdfff808080808080ffff04ffff0bffff0101ff
    8205df80ff808080808080ffff02ff2effff04ff02ffff04ff05ffff04ff17ff
    ff04ffff10ff819fff1780ffff04ff8205dfff80808080808080808080ff0180
    80ff018080ff8080808080808080ffff01ff088080ff0180ffff04ffff01ffff
    ff333eff42ff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c3
    85a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721
    e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd25
    31e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879
    b7152a6e7298a91ce119a63400ade7c5ffffff04ffff04ff18ffff04ffff0eff
    ff0163ff0b80ff808080ffff04ffff02ff1affff04ff02ffff04ff05ffff04ff
    0bffff04ff17ff808080808080ff2f8080ff04ff10ffff04ffff0bff81bcffff
    0bff2cffff0bff2cff81dcff0580ffff0bff2cffff0bff81fcffff0bff2cffff
    0bff2cff81dcffff0bffff0101ff0b8080ffff0bff2cff81dcff819c808080ff
    819c808080ffff04ff80ffff04ffff04ff17ff8080ff8080808080ffff0bffff
    0102ffff0bffff0101ff0580ffff0bffff0102ffff0bffff0101ff0b80ffff0b
    ffff0101ff17808080ffff02ffff03ffff09ff17ff2f80ff80ffff01ff04ffff
    02ff1affff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff17ff
    ff01ff01ff8080808080ffff04ffff0bffff0101ff1780ff808080808080ffff
    02ff2effff04ff02ffff04ff05ffff04ff0bffff04ffff10ff17ff0b80ffff04
    ff2fff808080808080808080ff0180ff04ff14ffff04ffff0112ffff04ff80ff
    ff04ffff0bff81bcffff0bff2cffff0bff2cff81dcff0580ffff0bff2cffff0b
    ff81fcffff0bff2cffff0bff2cff81dcffff0bffff0101ff0b8080ffff0bff2c
    ff81dcff819c808080ff819c808080ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    2c49bc36a8ec2f2703fddf92e4ae3dcbed849bb07cf6d3264f6714d04413acc0
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
