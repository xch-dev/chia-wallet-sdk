use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE: [u8; 1193] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff20ffff15ff8207efff8202ff8080ffff15
    ff8207ffff808080ffff01ff04ffff04ff4fffff04ffff10ff81afff8207ff80
    ffff04ff82016fffff04ff8202efff8203ef80808080ffff02ff12ffff04ff02
    ffff04ff0bffff04ffff0bffff0102ffff0bffff0101ff8202ff80ffff0bffff
    0102ffff0bffff0101ff8205ff80ffff0bffff0101ff8207ff808080ffff04ff
    8205ffffff04ffff04ffff02ff3effff04ff02ffff04ff05ffff04ffff02ff16
    ffff04ff02ffff04ff5fffff04ff81bfffff04ff82017fff808080808080ff80
    80808080ffff02ffff03ffff09ff8202ffff5f80ffff01ff04ffff02ff1affff
    04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff5fffff04ff81bf
    ffff04ffff10ff82017fff8207ff80ff808080808080ffff04ffff0bffff0101
    ff5f80ff808080808080ff8080ffff01ff02ffff03ff81bfffff01ff0880ffff
    01ff04ffff02ff1affff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ff
    ff04ff5fffff04ffff0101ffff04ff82017fff808080808080ffff04ffff0bff
    ff0101ff5f80ff808080808080ffff04ffff02ff1affff04ff02ffff04ff05ff
    ff04ffff02ff16ffff04ff02ffff04ff8202ffffff04ff80ffff04ff8207ffff
    808080808080ffff04ffff0bffff0101ff8202ff80ff808080808080ffff02ff
    2effff04ff02ffff04ff05ffff04ff17ffff04ffff10ff5fff1780ffff04ff82
    02ffff80808080808080808080ff018080ff018080ff8080808080808080ffff
    01ff088080ff0180ffff04ffff01ffffff333eff42ff02ffffa04bf5122f3445
    54c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184
    f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a128
    71fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102
    a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5
    ffffff04ffff04ff18ffff04ffff0effff0163ff0b80ff808080ffff04ffff02
    ff1affff04ff02ffff04ff05ffff04ff0bffff04ff17ff808080808080ff2f80
    80ff04ff10ffff04ffff0bff81bcffff0bff2cffff0bff2cff81dcff0580ffff
    0bff2cffff0bff81fcffff0bff2cffff0bff2cff81dcffff0bffff0101ff0b80
    80ffff0bff2cff81dcff819c808080ff819c808080ffff04ff80ffff04ffff04
    ff17ff8080ff8080808080ffff0bffff0102ffff0bffff0101ff0580ffff0bff
    ff0102ffff0bffff0101ff0b80ffff0bffff0101ff17808080ffff02ffff03ff
    ff09ff17ff2f80ff80ffff01ff04ffff02ff1affff04ff02ffff04ff05ffff04
    ffff02ff16ffff04ff02ffff04ff17ffff01ff01ff8080808080ffff04ffff0b
    ffff0101ff1780ff808080808080ffff02ff2effff04ff02ffff04ff05ffff04
    ff0bffff04ffff10ff17ff0b80ffff04ff2fff808080808080808080ff0180ff
    04ff14ffff04ffff0112ffff04ff80ffff04ffff0bff81bcffff0bff2cffff0b
    ff2cff81dcff0580ffff0bff2cffff0bff81fcffff0bff2cffff0bff2cff81dc
    ffff0bffff0101ff0b8080ffff0bff2cff81dcff819c808080ff819c808080ff
    8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    54f50fce54ba2bb77571c639221fa8ce3965560f2f89b6866dc00e5fc587b13e
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
