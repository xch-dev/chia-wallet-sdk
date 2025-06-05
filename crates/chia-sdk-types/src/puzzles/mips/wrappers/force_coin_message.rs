use std::borrow::Cow;

use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForceCoinMessageMod;

impl Mod for ForceCoinMessageMod {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&FORCE_COIN_MESSAGE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        FORCE_COIN_MESSAGE_PUZZLE_HASH
    }
}

pub const FORCE_COIN_MESSAGE_PUZZLE: [u8; 127] = hex!(
    "
    ff02ffff01ff02ff06ffff04ff02ffff04ff05ff80808080ffff04ffff01ff42
    ff02ffff03ffff02ffff03ffff09ff11ff0480ffff01ff02ffff03ffff18ff29
    ffff010780ffff01ff0101ff8080ff0180ff8080ff0180ffff0105ffff01ff04
    ff09ffff02ff06ffff04ff02ffff04ff0dff808080808080ff0180ff018080
    "
);

pub const FORCE_COIN_MESSAGE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "9618c96b30b96362f6c01716a11f76c630a786697d5bac92345f5ff90b882268"
));
