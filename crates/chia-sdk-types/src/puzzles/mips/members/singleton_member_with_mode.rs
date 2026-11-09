use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{SINGLETON_MEMBER_WITH_MODE, SINGLETON_MEMBER_WITH_MODE_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS singleton member puzzle, which lets a singleton approve a
/// delegated puzzle hash.
///
/// It outputs a `RECEIVE_MESSAGE` condition with the curried mode whose message is the delegated
/// puzzle hash, sent by the singleton's full puzzle hash. So the singleton must send that message
/// in the same block.
///
/// Like all MIPS members, it receives the delegated puzzle hash as the first value of its solution
/// from the layer above, followed by the member's own solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct SingletonMemberWithMode {
    /// The singleton struct of the singleton, which contains its launcher id.
    pub singleton_struct: SingletonStruct,
    /// The message mode. The sender bits (the upper 3) should be `0b010`, since the singleton is
    /// identified only by its puzzle hash. See [`MessageFlags`](crate::MessageFlags).
    pub mode: u8,
}

impl SingletonMemberWithMode {
    /// Creates a new [`SingletonMemberWithMode`] for the singleton with the given launcher id and
    /// message mode.
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

/// The member solution for [`SingletonMemberWithMode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct SingletonMemberWithModeSolution {
    /// The current inner puzzle hash of the singleton, used to compute its full puzzle hash.
    pub singleton_inner_puzzle_hash: Bytes32,
    /// The amount of the singleton. The puzzle doesn't use it.
    pub singleton_amount: u64,
}

impl SingletonMemberWithModeSolution {
    /// Creates a new [`SingletonMemberWithModeSolution`] from the singleton's inner puzzle hash and
    /// amount.
    pub fn new(singleton_inner_puzzle_hash: Bytes32, singleton_amount: u64) -> Self {
        Self {
            singleton_inner_puzzle_hash,
            singleton_amount,
        }
    }
}
