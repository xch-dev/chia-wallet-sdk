use std::borrow::Cow;

use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

// run '(mod (Inner_Puzzle_Hash) Inner_Puzzle_Hash)' -d
pub const XCH_CAT_MAKER_PUZZLE: [u8; 1] = hex!(
    "
    02
    "
);

pub const XCH_CAT_MAKER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222
    "
));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchCatMaker {}

impl Mod for XchCatMaker {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCH_CAT_MAKER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCH_CAT_MAKER_PUZZLE_HASH
    }
}
