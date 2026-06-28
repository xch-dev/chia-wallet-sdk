use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{SINGLETON_MEMBER_WITH_MODE, SINGLETON_MEMBER_WITH_MODE_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct SingletonMemberWithMode {
    pub singleton_struct: SingletonStruct,
    pub mode: u8,
}

impl SingletonMemberWithMode {
    pub fn new(launcher_id: Bytes32, mode: u8) -> Self {
        Self {
            singleton_struct: SingletonStruct::new(launcher_id),
            mode,
        }
    }
}

impl Mod for SingletonMemberWithMode {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SINGLETON_MEMBER_WITH_MODE)
    }

    fn mod_hash() -> TreeHash {
        SINGLETON_MEMBER_WITH_MODE_HASH.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct SingletonMemberWithModeSolution {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub singleton_amount: u64,
}

impl SingletonMemberWithModeSolution {
    pub fn new(singleton_inner_puzzle_hash: Bytes32, singleton_amount: u64) -> Self {
        Self {
            singleton_inner_puzzle_hash,
            singleton_amount,
        }
    }
}
