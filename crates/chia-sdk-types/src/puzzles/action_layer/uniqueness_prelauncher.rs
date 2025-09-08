use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const UNIQUENESS_PRELAUNCHER_PUZZLE: [u8; 59] = hex!(
    "
    ff02ffff01ff04ffff04ff04ffff04ff05ffff01ff01808080ffff04ffff04ff
    06ffff04ff0bff808080ff808080ffff04ffff01ff333eff018080
    "
);

pub const UNIQUENESS_PRELAUNCHER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    851c3d39cef84cfd9449afcaeff5f50d1be9371d8b7d6057ac318bec553a1a9f
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct UniquenessPrelauncher1stCurryArgs {
    pub launcher_puzzle_hash: Bytes32,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct UniquenessPrelauncher2ndCurryArgs<V> {
    pub value: V,
}

impl Mod for UniquenessPrelauncher1stCurryArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&UNIQUENESS_PRELAUNCHER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        UNIQUENESS_PRELAUNCHER_PUZZLE_HASH
    }
}
