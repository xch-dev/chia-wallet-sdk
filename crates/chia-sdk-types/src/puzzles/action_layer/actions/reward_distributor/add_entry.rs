use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE: [u8; 581] = hex!(
    "
    ff02ffff01ff04ffff04ff819fffff04ff82015fffff04ffff10ff8202dfff82
    017f80ffff04ff8205dfff8207df80808080ffff04ffff04ff1cffff04ffff01
    12ffff04ffff0effff0161ffff0bffff0102ffff0bffff0101ff81bf80ffff0b
    ffff0101ff82017f808080ffff04ffff0bff56ffff0bff0affff0bff0aff66ff
    0580ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ff0b80ffff0bff0a
    ffff0bff76ffff0bff0affff0bff0aff66ff8201ff80ffff0bff0aff66ff4680
    8080ff46808080ff46808080ff8080808080ffff04ffff02ff1effff04ff02ff
    ff04ff17ffff04ffff0bffff0102ffff0bffff0101ff81bf80ffff0bffff0102
    ffff0bffff0101ff8209df80ffff0bffff0101ff82017f808080ffff04ff81bf
    ff808080808080ffff04ffff04ff08ffff04ffff10ff820bdfff2f80ff808080
    ff8080808080ffff04ffff01ffff55ff3343ff02ffffffa04bf5122f344554c5
    3bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f326
    23d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fe
    e210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5
    dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04
    ff14ffff04ffff0bff56ffff0bff0affff0bff0aff66ff0580ffff0bff0affff
    0bff76ffff0bff0affff0bff0aff66ffff0bffff0101ff0b8080ffff0bff0aff
    66ff46808080ff46808080ffff04ff80ffff04ffff04ff17ff8080ff80808080
    80ff018080
    "
);

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    2674326f7f9fd76a08980466edec6b26b4a20e98d4c56a82d1938500835cde60
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
