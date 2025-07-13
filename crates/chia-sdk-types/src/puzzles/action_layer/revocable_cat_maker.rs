use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{CAT_PUZZLE_HASH, REVOCATION_LAYER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REVOCABLE_CAT_MAKER_PUZZLE: [u8; 419] = hex!(
    "
    ff02ffff01ff0bff16ffff0bff04ffff0bff04ff1aff0980ffff0bff04ffff0b
    ff1effff0bff04ffff0bff04ff1affff0bffff0101ff098080ffff0bff04ffff
    0bff1effff0bff04ffff0bff04ff1aff0b80ffff0bff04ffff0bff1effff0bff
    04ffff0bff04ff1affff0bff16ffff0bff04ffff0bff04ff1aff1580ffff0bff
    04ffff0bff1effff0bff04ffff0bff04ff1affff0bffff0101ff158080ffff0b
    ff04ffff0bff1effff0bff04ffff0bff04ff1aff1d80ffff0bff04ffff0bff1e
    ffff0bff04ffff0bff04ff1affff0bffff0101ff178080ffff0bff04ff1aff12
    808080ff12808080ff12808080ff1280808080ffff0bff04ff1aff12808080ff
    12808080ff12808080ff12808080ffff04ffff01ff02ffffa04bf5122f344554
    c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f3
    2623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871
    fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8
    d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff
    018080
    "
);

pub const REVOCABLE_CAT_MAKER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    040753f82d91f7d6640cbd1ca0a53a5988a423808198725603eaf0db3280703b
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RevocableCatMakerStruct {
    pub cat_mod_hash: Bytes32,
    pub revocation_layer_mod_hash: Bytes32,
    #[clvm(rest)]
    pub hidden_puzzle_hash_hash: Bytes32,
}

impl RevocableCatMakerStruct {
    pub fn new(hidden_puzzle_hash_hash: TreeHash) -> Self {
        Self {
            cat_mod_hash: CAT_PUZZLE_HASH.into(),
            revocation_layer_mod_hash: REVOCATION_LAYER_HASH.into(),
            hidden_puzzle_hash_hash: hidden_puzzle_hash_hash.into(),
        }
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RevocableCatMakerArgs {
    pub mod_struct: RevocableCatMakerStruct,
    pub tail_hash_hash: Bytes32,
}

impl RevocableCatMakerArgs {
    pub fn new(tail_hash_hash: TreeHash, hidden_puzzle_hash_hash: TreeHash) -> Self {
        Self {
            mod_struct: RevocableCatMakerStruct::new(hidden_puzzle_hash_hash),
            tail_hash_hash: tail_hash_hash.into(),
        }
    }
}

impl Mod for RevocableCatMakerArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REVOCABLE_CAT_MAKER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REVOCABLE_CAT_MAKER_PUZZLE_HASH
    }
}
