use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITHOUT_APPROVAL_PUZZLE: [u8; 760] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff22ffff22ffff09ffff12ffff11ff8204ef
    ff8207ff80ff8205ff80ffff10ffff12ff81bfff1780ff8202ff8080ffff15ff
    8202ffffff0181ff8080ffff15ff17ff8202ff8080ffff21ffff15ff81bfff0b
    80ffff09ff81bfff0b808080ffff01ff04ffff04ff4fffff04ffff11ff81afff
    81bf80ffff04ff82016fffff04ffff04ff8204efffff10ff8206efff8202ff80
    80ff8203ef80808080ffff04ffff04ffff0142ffff04ffff0112ffff04ff80ff
    ff04ffff02ff04ffff04ff0affff04ff05ffff04ffff0bffff0101ffff02ff0e
    ffff04ffff04ff5fff82017f80ffff04ff8207ffff8205ff80808080ff808080
    8080ff8080808080ffff04ffff04ffff0133ffff04ffff02ff04ffff04ff0aff
    ff04ff05ffff04ffff0bffff0101ffff02ff0effff04ffff04ffff10ff5fffff
    010180ff82017f80ffff04ff8204efff8205ff80808080ff8080808080ffff04
    ff80ffff04ffff04ff82017fff8080ff8080808080ffff04ffff04ffff013eff
    ff04ffff0effff0170ffff0bffff0102ffff0bffff0101ff82017f80ffff0bff
    ff0101ff81bf808080ff808080ffff04ffff04ffff0181d6ffff04ffff0133ff
    ff04ff82017fffff04ff81bfffff04ffff04ff82017fff8080ff808080808080
    ff808080808080ffff01ff088080ff0180ffff04ffff04ffff01ff0bffff0102
    ffff0bffff0182010280ffff0bffff0102ffff0bffff0102ffff0bffff018201
    0180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff01
    0180808080ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff
    0182010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580
    ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080
    ffff01ff0bffff018201018080ff0180ffff01ff0bffff0102ffff0bffff0101
    ff0480ffff0bffff0102ffff0bffff0101ff0680ffff0bffff0102ffff0bffff
    0101ff0580ffff0bffff0101ff07808080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITHOUT_APPROVAL_PUZZLE_HASH: TreeHash =
    TreeHash::new(hex!(
        "
    2936969bd12dde18486ea4cf8f8d8bb3f05d11170d445b5c1a046539449e1896
    "
    ));

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITH_APPROVAL_PUZZLE: [u8; 833] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff22ffff22ffff09ffff12ffff11ff8204ef
    ff8207ff80ff8205ff80ffff10ffff12ff81bfff1780ff8202ff8080ffff15ff
    8202ffffff0181ff8080ffff15ff17ff8202ff8080ffff21ffff15ff81bfff0b
    80ffff09ff81bfff0b808080ffff01ff04ffff04ff4fffff04ffff11ff81afff
    81bf80ffff04ff82016fffff04ffff04ff8204efffff10ff8206efff8202ff80
    80ff8203ef80808080ffff04ffff04ffff0142ffff04ffff0112ffff04ff80ff
    ff04ffff02ff04ffff04ff0affff04ff05ffff04ffff0bffff0101ffff02ff0e
    ffff04ffff04ff5fff82017f80ffff04ff8207ffff8205ff80808080ff808080
    8080ff8080808080ffff04ffff04ffff0133ffff04ffff02ff04ffff04ff0aff
    ff04ff05ffff04ffff0bffff0101ffff02ff0effff04ffff04ffff10ff5fffff
    010180ff82017f80ffff04ff8204efff8205ff80808080ff8080808080ffff04
    ff80ffff04ffff04ff82017fff8080ff8080808080ffff04ffff04ffff013eff
    ff04ffff0effff0170ffff0bffff0102ffff0bffff0101ff82017f80ffff0bff
    ff0101ff81bf808080ff808080ffff04ffff04ffff0181d6ffff04ffff0133ff
    ff04ff82017fffff04ff81bfffff04ffff04ff82017fff8080ff808080808080
    ffff04ffff04ffff0143ffff04ffff0112ffff04ffff0effff0170ffff0bffff
    0102ffff0bffff0101ff81bf80ffff0bffff0101ff8202ff808080ffff04ff82
    017fff8080808080ff80808080808080ffff01ff088080ff0180ffff04ffff04
    ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff01
    02ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff
    078080ffff0bffff010180808080ffff04ffff01ff02ffff03ff03ffff01ff0b
    ffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0bff
    ff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff
    0bffff010180808080ffff01ff0bffff018201018080ff0180ffff01ff0bffff
    0102ffff0bffff0101ff0480ffff0bffff0102ffff0bffff0101ff0680ffff0b
    ffff0102ffff0bffff0101ff0580ffff0bffff0101ff07808080808080ff0180
    80
    "
);

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITH_APPROVAL_PUZZLE_HASH: TreeHash =
    TreeHash::new(hex!(
        "
        4b405cbb838b10d4996816aa2b65d0f4a8add9fb7871f9aa8ccf3ba57a91ea5d
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
