use std::borrow::Cow;

use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const ANY_METADATA_UPDATER: [u8; 23] = hex!(
    "
    ff04ffff04ff0bffff04ff05ff808080ffff01ff808080
    "
);

pub const ANY_METADATA_UPDATER_HASH: TreeHash = TreeHash::new(hex!(
    "
    9f28d55242a3bd2b3661c38ba8647392c26bb86594050ea6d33aad1725ca3eea
    "
));

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct AnyMetadataUpdater {}

impl Mod for AnyMetadataUpdater {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&ANY_METADATA_UPDATER)
    }

    fn mod_hash() -> TreeHash {
        ANY_METADATA_UPDATER_HASH
    }
}
