use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE: [u8; 590] = hex!(
    "
    ff02ffff01ff04ffff04ff819fffff04ff82015fffff04ffff10ff8202dfff82
    03bf80ffff04ff8205dfffff04ff820bdfff808080808080ffff04ffff04ff1c
    ffff04ffff0112ffff04ffff0effff0161ffff0bffff0102ffff0bffff0101ff
    8202bf80ffff0bffff0101ff8203bf808080ffff04ffff0bff56ffff0bff0aff
    ff0bff0aff66ff0580ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ff
    0b80ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ff82013f80ffff0b
    ff0aff66ff46808080ff46808080ff46808080ff8080808080ffff04ffff02ff
    1effff04ff02ffff04ff17ffff04ffff0bffff0102ffff0bffff0101ff8202bf
    80ffff0bffff0102ffff0bffff0101ff8209df80ffff0bffff0101ff8203bf80
    8080ffff04ff8202bfff808080808080ffff04ffff04ff08ffff04ffff10ff82
    13dfff2f80ff808080ff8080808080ffff04ffff01ffff55ff3343ff02ffffff
    a04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c778545
    9aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718b
    a7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f680
    6923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce1
    19a63400ade7c5ff04ff14ffff04ffff0bff56ffff0bff0affff0bff0aff66ff
    0580ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ffff0bffff0101ff
    0b8080ffff0bff0aff66ff46808080ff46808080ffff04ff80ffff04ffff04ff
    17ff8080ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    90eef279e5389305ed3ff673fa5c766258e5ea04ff7abcec1ed551060bca8aa0
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
    pub manager_singleton_inner_puzzle_hash: Bytes32,
    pub entry_payout_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub entry_shares: u64,
}

impl Mod for RewardDistributorAddEntryActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE_HASH
    }
}
