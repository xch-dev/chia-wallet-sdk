use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::CAT_PUZZLE_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const DEFAULT_CAT_MAKER_PUZZLE: [u8; 217] = hex!(
    // Rue
    "
    ff02ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0bff
    ff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff
    02ffff04ffff0bffff0101ff0580ffff04ff0bffff04ff17ff808080808080ff
    ff0bffff010180808080ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102
    ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0bffff018201
    0180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff01
    0180808080ffff01ff0bffff018201018080ff0180ff018080
    "
);

pub const DEFAULT_CAT_MAKER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    d6693f7aacfeb7e6901cd71a00f5e4b318d73234fbffd5b4d84da2c1371b3181
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DefaultCatMakerArgs {
    pub cat_mod_hash: Bytes32,
    pub tail_hash_hash: Bytes32,
}

impl DefaultCatMakerArgs {
    pub fn new(tail_hash_hash: Bytes32) -> Self {
        Self {
            cat_mod_hash: CAT_PUZZLE_HASH.into(),
            tail_hash_hash,
        }
    }
}

impl Mod for DefaultCatMakerArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&DEFAULT_CAT_MAKER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        DEFAULT_CAT_MAKER_PUZZLE_HASH
    }
}
