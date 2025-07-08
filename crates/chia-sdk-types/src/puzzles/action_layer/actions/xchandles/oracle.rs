use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const XCHANDLES_ORACLE_PUZZLE: [u8; 571] = hex!(
    "
    ff02ffff01ff04ff0bffff02ff16ffff04ff02ffff04ff05ffff04ffff02ff2e
    ffff04ff02ffff04ff17ff80808080ff808080808080ffff04ffff01ffffff33
    3eff4202ffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385
    a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e8
    78a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531
    e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7
    152a6e7298a91ce119a63400ade7c5ff04ff10ffff04ffff0bff52ffff0bff1c
    ffff0bff1cff62ff0580ffff0bff1cffff0bff72ffff0bff1cffff0bff1cff62
    ffff0bffff0101ff0b8080ffff0bff1cff62ff42808080ff42808080ffff04ff
    80ffff04ffff04ff05ff8080ff8080808080ffff04ffff02ff3effff04ff02ff
    ff04ff05ffff04ff0bff8080808080ffff04ffff02ff1affff04ff02ffff04ff
    05ffff04ff0bff8080808080ffff04ffff04ff18ffff04ffff0effff016fff0b
    80ff808080ff80808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ff
    ff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04
    ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04ff14ffff04ff
    ff0112ffff04ff80ffff04ffff0bff52ffff0bff1cffff0bff1cff62ff0580ff
    ff0bff1cffff0bff72ffff0bff1cffff0bff1cff62ffff0bffff0101ff0b8080
    ffff0bff1cff62ff42808080ff42808080ff8080808080ff018080
    "
);

pub const XCHANDLES_ORACLE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    1ba03341b929f37687610644f24a0cd36cb6ef019dc7289a0c2172d61482c23c
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesOracleActionArgs {
    pub slot_1st_curry_hash: Bytes32,
}

impl Mod for XchandlesOracleActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_ORACLE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_ORACLE_PUZZLE_HASH
    }
}
