use std::borrow::Cow;

use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Serialized CAT maker for XCH: it returns the inner puzzle hash unchanged, so payments
/// are made in XCH rather than a CAT.
// run '(mod (Inner_Puzzle_Hash) Inner_Puzzle_Hash)' -d
pub const XCH_CAT_MAKER_PUZZLE: [u8; 1] = hex!(
    "
    02
    "
);

/// Tree hash of [`XCH_CAT_MAKER_PUZZLE`].
pub const XCH_CAT_MAKER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222
    "
));

/// Marker for [`XCH_CAT_MAKER_PUZZLE`], which takes no curried arguments.
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
