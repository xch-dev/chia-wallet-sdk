use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{
    EVERYTHING_WITH_SINGLETON, EVERYTHING_WITH_SINGLETON_HASH, SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct EverythingWithSingletonTailArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_struct_hash: Bytes32,
    pub nonce: usize,
}

impl EverythingWithSingletonTailArgs {
    pub fn new(launcher_id: Bytes32, nonce: usize) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_struct_hash: SingletonStruct::new(launcher_id).tree_hash().into(),
            nonce,
        }
    }
}

impl Mod for EverythingWithSingletonTailArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&EVERYTHING_WITH_SINGLETON)
    }

    fn mod_hash() -> TreeHash {
        EVERYTHING_WITH_SINGLETON_HASH.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct EverythingWithSingletonTailSolution {
    pub singleton_inner_puzzle_hash: Bytes32,
}

impl EverythingWithSingletonTailSolution {
    pub fn new(singleton_inner_puzzle_hash: Bytes32) -> Self {
        Self {
            singleton_inner_puzzle_hash,
        }
    }
}
