use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{CAT_PUZZLE_HASH, REVOCATION_LAYER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REVOCABLE_CAT_MAKER_PUZZLE: [u8; 295] = hex!(
    // Rue
    "
    ff02ffff01ff02ff04ffff04ff06ffff04ff09ffff04ffff0bffff0101ff0980
    ffff04ff0bffff04ffff02ff04ffff04ff06ffff04ff15ffff04ffff0bffff01
    01ff1580ffff04ff1dffff04ffff0bffff0101ff1780ff80808080808080ff80
    808080808080ffff04ffff04ffff01ff0bffff0102ffff0bffff0182010280ff
    ff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff01
    02ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff02ff
    ff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff
    0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff
    04ff02ff078080ffff0bffff010180808080ffff01ff0bffff018201018080ff
    018080ff018080
    "
);

pub const REVOCABLE_CAT_MAKER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    f1b280e985d067fa79a86eca600d8fe9ca62bf2eaa5346a6ddaf23c7773ee955
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
