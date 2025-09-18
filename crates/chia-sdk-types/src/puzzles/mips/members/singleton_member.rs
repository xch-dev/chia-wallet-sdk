use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{SINGLETON_MEMBER, SINGLETON_MEMBER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct SingletonMember {
    pub singleton_struct: SingletonStruct,
}

impl SingletonMember {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self {
            singleton_struct: SingletonStruct::new(launcher_id),
        }
    }
}

impl Mod for SingletonMember {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SINGLETON_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        SINGLETON_MEMBER_HASH.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct SingletonMemberSolution {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub singleton_amount: u64,
}

impl SingletonMemberSolution {
    pub fn new(singleton_inner_puzzle_hash: Bytes32, singleton_amount: u64) -> Self {
        Self {
            singleton_inner_puzzle_hash,
            singleton_amount,
        }
    }
}
