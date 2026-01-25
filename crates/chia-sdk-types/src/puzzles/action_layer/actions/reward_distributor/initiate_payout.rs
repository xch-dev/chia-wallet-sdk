use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITHOUT_APPROVAL_PUZZLE: [u8; 771] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ffff12ffff11ff8204efff8203ff80ff
    8202ff80ffff10ffff12ff5fff1780ff82017f8080ffff15ff82017fffff0181
    ff80ffff15ff17ff82017f80ffff20ffff15ff0bff5f808080ffff01ff04ffff
    04ff4fffff04ffff11ff81afff5f80ff81ef8080ffff04ffff02ff1effff04ff
    02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff81bfffff04ff8203ff
    ffff04ff8202ffff808080808080ff8080808080ffff04ffff02ff1affff04ff
    02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff81bfffff04ffff11ff
    8204efff82017f80ffff04ff8202ffff808080808080ffff04ff81bfff808080
    808080ffff04ffff04ff18ffff04ffff0effff0170ffff0bffff0102ffff0bff
    ff0101ff81bf80ffff0bffff0101ff5f808080ff808080ffff04ffff04ffff01
    81d6ffff04ff10ffff04ff81bfffff04ff5fffff04ffff04ff81bfff8080ff80
    8080808080ff808080808080ffff01ff088080ff0180ffff04ffff01ffffff33
    3eff4202ffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385
    a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e8
    78a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531
    e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7
    152a6e7298a91ce119a63400ade7c5ff04ff10ffff04ffff0bff52ffff0bff1c
    ffff0bff1cff62ff0580ffff0bff1cffff0bff72ffff0bff1cffff0bff1cff62
    ffff0bffff0101ff0b8080ffff0bff1cff62ff42808080ff42808080ffff04ff
    80ffff04ffff04ff17ff8080ff8080808080ffff0bffff0102ffff0bffff0101
    ff0580ffff0bffff0102ffff0bffff0101ff0b80ffff0bffff0101ff17808080
    ff04ff14ffff04ffff0112ffff04ff80ffff04ffff0bff52ffff0bff1cffff0b
    ff1cff62ff0580ffff0bff1cffff0bff72ffff0bff1cffff0bff1cff62ffff0b
    ffff0101ff0b8080ffff0bff1cff62ff42808080ff42808080ff8080808080ff
    018080
    "
);

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITHOUT_APPROVAL_PUZZLE_HASH: TreeHash =
    TreeHash::new(hex!(
        "
    5e2730f03dc1c06ba55499e2091cae832b5078ee3b7731e9e9426beffcc7a6b7
    "
    ));

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITH_APPROVAL_PUZZLE: [u8; 842] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ffff12ffff11ff8204efff8203ff80ff
    8202ff80ffff10ffff12ff5fff1780ff82017f8080ffff15ff82017fffff0181
    ff80ffff15ff17ff82017f80ffff20ffff15ff0bff5f808080ffff01ff04ffff
    04ff4fffff04ffff11ff81afff5f80ff81ef8080ffff04ffff02ff3effff04ff
    02ffff04ff05ffff04ffff02ff2effff04ff02ffff04ff81bfffff04ff8203ff
    ffff04ff8202ffff808080808080ff8080808080ffff04ffff02ff16ffff04ff
    02ffff04ff05ffff04ffff02ff2effff04ff02ffff04ff81bfffff04ffff11ff
    8204efff82017f80ffff04ff8202ffff808080808080ffff04ff81bfff808080
    808080ffff04ffff04ff18ffff04ffff0effff0170ffff0bffff0102ffff0bff
    ff0101ff81bf80ffff0bffff0101ff5f808080ff808080ffff04ffff04ffff01
    81d6ffff04ff10ffff04ff81bfffff04ff5fffff04ffff04ff81bfff8080ff80
    8080808080ffff04ffff04ff14ffff04ffff0112ffff04ffff0effff0170ffff
    0bffff0102ffff0bffff0101ff5f80ffff0bffff0101ff82017f808080ffff04
    ff81bfff8080808080ff80808080808080ffff01ff088080ff0180ffff04ffff
    01ffffff333eff4342ffff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1
    600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5
    709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea1
    94581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e
    8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff04ff10ffff04ffff0b
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

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITH_APPROVAL_PUZZLE_HASH: TreeHash =
    TreeHash::new(hex!(
        "
        a80537d109668e8880fdf79cca4f5a8060f46ccf32d7261e9310642e3684bc7c
        "
    ));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorInitiatePayoutWithoutApprovalActionArgs {
    pub entry_slot_1st_curry_hash: Bytes32,
    pub payout_threshold: u64,
    pub precision: u64,
}

impl Mod for RewardDistributorInitiatePayoutWithoutApprovalActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITHOUT_APPROVAL_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITHOUT_APPROVAL_PUZZLE_HASH
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorInitiatePayoutWithApprovalActionArgs {
    pub entry_slot_1st_curry_hash: Bytes32,
    pub payout_threshold: u64,
    pub precision: u64,
}

impl Mod for RewardDistributorInitiatePayoutWithApprovalActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITH_APPROVAL_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITH_APPROVAL_PUZZLE_HASH
    }
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorInitiatePayoutActionSolution {
    pub entry_payout_amount: u64,
    pub entry_payout_puzzle_hash: Bytes32,
    pub payout_rounding_error: u128,
    pub entry_shares: u64,
    #[clvm(rest)]
    pub entry_initial_cumulative_payout: u128,
}
