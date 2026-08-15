use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE: [u8; 533] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff15ff82017fff8080ffff01ff04ffff04ff819fff
    ff04ff82015fffff04ffff10ff8202dfff82017f80ff8203df808080ffff04ff
    ff04ffff0143ffff04ffff0112ffff04ffff0effff0161ffff0bffff0102ffff
    0bffff0101ff81bf80ffff0bffff0101ff82017f808080ffff04ffff02ff04ff
    ff04ff06ffff04ff05ffff04ff0bffff04ff8201ffff808080808080ff808080
    8080ffff04ffff04ffff0133ffff04ffff02ff04ffff04ff06ffff04ff17ffff
    04ffff0bffff0101ffff0bffff0102ffff0bffff010180ffff0bffff0102ffff
    0bffff0101ff81bf80ffff0bffff0102ffff0bffff0101ff8209df80ffff0bff
    ff0101ff82017f8080808080ff8080808080ffff04ff80ffff04ffff04ff81bf
    ff8080ff8080808080ffff04ffff04ffff0155ffff04ffff10ff820bdfff2f80
    ff808080ff8080808080ffff01ff088080ff0180ffff04ffff04ffff01ff0bff
    ff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff0102ffff0bffff
    0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0b
    ffff010180808080ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff
    0182010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580
    ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080
    ffff01ff0bffff018201018080ff018080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    9a25633bc5b34abc08bf75b62ad5d44caa37270065161c1800189aabe2ae45ec
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorAddEntryActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub manager_singleton_struct_hash: Bytes32,
    pub entry_slot_1st_curry_hash: Bytes32,
    pub max_second_offset: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorAddEntryActionSolution {
    pub entry_payout_puzzle_hash: Bytes32,
    pub entry_shares: u64,
    #[clvm(rest)]
    pub manager_singleton_inner_puzzle_hash: Bytes32,
}

impl Mod for RewardDistributorAddEntryActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE_HASH
    }
}
