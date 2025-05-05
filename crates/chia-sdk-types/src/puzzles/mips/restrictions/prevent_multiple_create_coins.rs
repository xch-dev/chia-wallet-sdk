use std::borrow::Cow;

use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreventMultipleCreateCoinsMod;

impl Mod for PreventMultipleCreateCoinsMod {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&PREVENT_MULTIPLE_CREATE_COINS_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH
    }
}

pub const PREVENT_MULTIPLE_CREATE_COINS_PUZZLE: [u8; 143] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ffff02ff06ffff04ff02ffff04ff05ffff01ff
    8080808080ffff010180ffff0105ffff01ff088080ff0180ffff04ffff01ff33
    ff02ffff03ff05ffff01ff02ff06ffff04ff02ffff04ff0dffff04ffff02ffff
    03ffff09ff11ff0480ffff01ff10ff0bffff010180ffff010b80ff0180ff8080
    808080ffff010b80ff0180ff018080
    "
);

pub const PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "93b8c8abeab8f6bdba4acb49ed49362ecba94b703a48b15c8784f966547b7846"
));
