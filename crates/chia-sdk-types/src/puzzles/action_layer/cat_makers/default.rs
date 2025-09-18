use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::CAT_PUZZLE_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const DEFAULT_CAT_MAKER_PUZZLE: [u8; 283] = hex!(
    "
    ff02ffff01ff0bff16ffff0bff04ffff0bff04ff1aff0580ffff0bff04ffff0b
    ff1effff0bff04ffff0bff04ff1affff0bffff0101ff058080ffff0bff04ffff
    0bff1effff0bff04ffff0bff04ff1aff0b80ffff0bff04ffff0bff1effff0bff
    04ffff0bff04ff1aff1780ffff0bff04ff1aff12808080ff12808080ff128080
    80ff12808080ffff04ffff01ff02ffffa04bf5122f344554c53bde2ebb8cd2b7
    e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb
    99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291e
    aea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb
    1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff018080
    "
);

pub const DEFAULT_CAT_MAKER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    0370e9c0343398cbe3487fb93d4aa24357005cdd67894e1cbae14772e778a75a
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
