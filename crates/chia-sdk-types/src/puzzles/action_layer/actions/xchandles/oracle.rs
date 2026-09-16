use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const XCHANDLES_ORACLE_PUZZLE: [u8; 549] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff04ff17ffff04ffff04ffff0142ffff04ffff0112ff
    ff04ff80ffff04ffff0bffff0102ffff0bffff0182010280ffff0bffff0102ff
    ff0bffff0102ffff0bffff0182010180ff0b80ffff0bffff0102ffff02ff15ff
    ff04ff15ffff04ffff0bffff0101ff0280ff80808080ffff0bffff0101808080
    80ff8080808080ffff04ffff02ff1dffff04ff15ffff04ff0bffff02ff09ffff
    04ff09ffff04ffff10ff2fffff010180ff3f808080808080ffff04ffff04ffff
    013effff04ffff0effff016fff0280ff808080ff8080808080ffff04ffff02ff
    04ffff04ff04ff0f8080ff018080ffff04ffff04ffff01ff02ffff03ffff07ff
    0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080ffff02ff02ffff
    04ff02ff07808080ffff01ff0bffff0101ff038080ff0180ffff04ffff01ff02
    ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ff
    ff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ff
    ff04ff02ff078080ffff0bffff010180808080ffff01ff0bffff018201018080
    ff0180ffff01ff04ffff0133ffff04ffff0bffff0102ffff0bffff0182010280
    ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff
    0102ffff02ff02ffff04ff02ffff04ffff0bffff0101ff0780ff80808080ffff
    0bffff010180808080ffff04ff80ffff04ffff04ff05ff8080ff808080808080
    80ff018080
    "
);

pub const XCHANDLES_ORACLE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    f6f82232ff1dead535602b757dcbe21ee6efb38ff01a0500e59c52161a582c1b
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesOracleActionArgs {
    pub handle_slot_1st_curry_hash: Bytes32,
}

impl Mod for XchandlesOracleActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_ORACLE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_ORACLE_PUZZLE_HASH
    }
}
