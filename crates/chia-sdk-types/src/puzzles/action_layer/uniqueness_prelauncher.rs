use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const UNIQUENESS_PRELAUNCHER_PUZZLE: [u8; 45] = hex!(
    "
    ff04ffff04ffff0133ffff04ff02ffff01ff01808080ffff04ffff04
    ffff013effff04ff05ff808080ff808080
    "
);

pub const UNIQUENESS_PRELAUNCHER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    e7fa797a8537895c571ae9556abdd2ebaba2d36957f2c63f37aba130f8f0c5ff
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
