use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE: [u8; 505] = hex!(
    // Rue
    "
    ff02ffff01ff04ffff04ff819fffff04ff82015fffff04ffff10ff8202dfff82
    017f80ff8203df808080ffff04ffff04ffff0143ffff04ffff0112ffff04ffff
    0effff0161ffff0bffff0102ffff0bffff0101ff81bf80ffff0bffff0101ff82
    017f808080ffff04ffff02ff04ffff04ff06ffff04ff05ffff04ff0bffff04ff
    8201ffff808080808080ff8080808080ffff04ffff04ffff0133ffff04ffff02
    ff04ffff04ff06ffff04ff17ffff04ffff0bffff0101ffff0bffff0102ffff0b
    ffff010180ffff0bffff0102ffff0bffff0101ff81bf80ffff0bffff0102ffff
    0bffff0101ff8209df80ffff0bffff0101ff82017f8080808080ff8080808080
    ffff04ff80ffff04ffff04ff81bfff8080ff8080808080ffff04ffff04ffff01
    55ffff04ffff10ff820bdfff2f80ff808080ff8080808080ffff04ffff04ffff
    01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff0102ff
    ff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff0780
    80ffff0bffff010180808080ffff01ff02ffff03ff03ffff01ff0bffff0102ff
    ff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0bffff01820101
    80ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff0101
    80808080ffff01ff0bffff018201018080ff018080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    a0d70d0f2ba82613b43593b53b610abde3008661d0f1f90c90e23740229c3d96
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
