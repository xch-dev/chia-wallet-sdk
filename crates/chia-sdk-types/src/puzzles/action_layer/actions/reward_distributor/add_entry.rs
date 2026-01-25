use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE: [u8; 587] = hex!(
    "
    ff02ffff01ff04ffff04ff819fffff04ff82015fffff04ffff10ff8202dfff82
    017f80ffff04ff8205dfffff04ff820bdfff808080808080ffff04ffff04ff1c
    ffff04ffff0112ffff04ffff0effff0161ffff0bffff0102ffff0bffff0101ff
    81bf80ffff0bffff0101ff82017f808080ffff04ffff0bff56ffff0bff0affff
    0bff0aff66ff0580ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ff0b
    80ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ff8201ff80ffff0bff
    0aff66ff46808080ff46808080ff46808080ff8080808080ffff04ffff02ff1e
    ffff04ff02ffff04ff17ffff04ffff0bffff0102ffff0bffff0101ff81bf80ff
    ff0bffff0102ffff0bffff0101ff8209df80ffff0bffff0101ff82017f808080
    ffff04ff81bfff808080808080ffff04ffff04ff08ffff04ffff10ff8213dfff
    2f80ff808080ff8080808080ffff04ffff01ffff55ff3343ff02ffffffa04bf5
    122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09d
    cf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ff
    a102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f6
    3222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a634
    00ade7c5ff04ff14ffff04ffff0bff56ffff0bff0affff0bff0aff66ff0580ff
    ff0bff0affff0bff76ffff0bff0affff0bff0aff66ffff0bffff0101ff0b8080
    ffff0bff0aff66ff46808080ff46808080ffff04ff80ffff04ffff04ff17ff80
    80ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    4197b2e680564f5f2740691ae93d1a833338224dbe3c3c1bfc9b19e9dddd6531
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
