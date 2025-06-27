use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{
    P2_SINGLETON, P2_SINGLETON_HASH, SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2SingletonArgs {
    pub singleton_mod_hash: Bytes32,
    pub launcher_id: Bytes32,
    pub launcher_puzzle_hash: Bytes32,
}

impl Mod for P2SingletonArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_SINGLETON)
    }

    fn mod_hash() -> TreeHash {
        P2_SINGLETON_HASH.into()
    }
}

impl P2SingletonArgs {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            launcher_id,
            launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
        }
    }

    pub fn curry_tree_hash(launcher_id: Bytes32) -> TreeHash {
        CurriedProgram {
            program: TreeHash::new(P2_SINGLETON_HASH),
            args: Self::new(launcher_id),
        }
        .tree_hash()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2SingletonSolution {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub my_id: Bytes32,
}
