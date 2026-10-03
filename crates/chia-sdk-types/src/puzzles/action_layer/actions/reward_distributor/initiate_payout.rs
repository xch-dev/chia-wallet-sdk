use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITHOUT_APPROVAL_PUZZLE: [u8; 751] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff22ffff22ffff09ffff12ffff11ff8204ef
    ff8207ff80ff8205ff80ffff10ffff12ff81bfff1780ff8202ff8080ffff15ff
    8202ffffff0181ff8080ffff15ff17ff8202ff8080ffff20ffff15ff0bff81bf
    808080ffff01ff04ffff04ff4fffff04ffff11ff81afff81bf80ffff04ff8201
    6fffff04ffff04ff8204efffff10ff8206efff8202ff8080ff8203ef80808080
    ffff04ffff04ffff0142ffff04ffff0112ffff04ff80ffff04ffff02ff04ffff
    04ff0affff04ff05ffff04ffff0bffff0101ffff02ff0effff04ffff04ff5fff
    82017f80ffff04ff8207ffff8205ff80808080ff8080808080ff8080808080ff
    ff04ffff04ffff0133ffff04ffff02ff04ffff04ff0affff04ff05ffff04ffff
    0bffff0101ffff02ff0effff04ffff04ffff10ff5fffff010180ff82017f80ff
    ff04ff8204efff8205ff80808080ff8080808080ffff04ff80ffff04ffff04ff
    82017fff8080ff8080808080ffff04ffff04ffff013effff04ffff0effff0170
    ffff0bffff0102ffff0bffff0101ff82017f80ffff0bffff0101ff81bf808080
    ff808080ffff04ffff04ffff0181d6ffff04ffff0133ffff04ff82017fffff04
    ff81bfffff04ffff04ff82017fff8080ff808080808080ff808080808080ffff
    01ff088080ff0180ffff04ffff04ffff01ff0bffff0102ffff0bffff01820102
    80ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bff
    ff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff04ff
    ff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0bff
    ff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff
    02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bffff0182
    01018080ff0180ffff01ff0bffff0102ffff0bffff0101ff0480ffff0bffff01
    02ffff0bffff0101ff0680ffff0bffff0102ffff0bffff0101ff0580ffff0bff
    ff0101ff07808080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITHOUT_APPROVAL_PUZZLE_HASH: TreeHash =
    TreeHash::new(hex!(
        "
    3ac00fa8db24e15d425af0624502a9ff7c588eeb2726bd5d7f83b39897484b66
    "
    ));

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITH_APPROVAL_PUZZLE: [u8; 824] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff22ffff22ffff09ffff12ffff11ff8204ef
    ff8207ff80ff8205ff80ffff10ffff12ff81bfff1780ff8202ff8080ffff15ff
    8202ffffff0181ff8080ffff15ff17ff8202ff8080ffff20ffff15ff0bff81bf
    808080ffff01ff04ffff04ff4fffff04ffff11ff81afff81bf80ffff04ff8201
    6fffff04ffff04ff8204efffff10ff8206efff8202ff8080ff8203ef80808080
    ffff04ffff04ffff0142ffff04ffff0112ffff04ff80ffff04ffff02ff04ffff
    04ff0affff04ff05ffff04ffff0bffff0101ffff02ff0effff04ffff04ff5fff
    82017f80ffff04ff8207ffff8205ff80808080ff8080808080ff8080808080ff
    ff04ffff04ffff0133ffff04ffff02ff04ffff04ff0affff04ff05ffff04ffff
    0bffff0101ffff02ff0effff04ffff04ffff10ff5fffff010180ff82017f80ff
    ff04ff8204efff8205ff80808080ff8080808080ffff04ff80ffff04ffff04ff
    82017fff8080ff8080808080ffff04ffff04ffff013effff04ffff0effff0170
    ffff0bffff0102ffff0bffff0101ff82017f80ffff0bffff0101ff81bf808080
    ff808080ffff04ffff04ffff0181d6ffff04ffff0133ffff04ff82017fffff04
    ff81bfffff04ffff04ff82017fff8080ff808080808080ffff04ffff04ffff01
    43ffff04ffff0112ffff04ffff0effff0170ffff0bffff0102ffff0bffff0101
    ff81bf80ffff0bffff0101ff8202ff808080ffff04ff82017fff8080808080ff
    80808080808080ffff01ff088080ff0180ffff04ffff04ffff01ff0bffff0102
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

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITH_APPROVAL_PUZZLE_HASH: TreeHash =
    TreeHash::new(hex!(
        "
        af5655ab9e43acce002073b7efc9a9a4ce6a5c3aabd215d594d9ec999d6c0492
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
    pub slot_counter: u64,
    pub entry_payout_amount: u64,
    pub entry_payout_puzzle_hash: Bytes32,
    pub payout_rounding_error: u128,
    pub entry_shares: u64,
    #[clvm(rest)]
    pub entry_initial_cumulative_payout: u128,
}
