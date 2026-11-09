use std::borrow::Cow;

use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{
    EVERYTHING_WITH_SINGLETON, EVERYTHING_WITH_SINGLETON_HASH, SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::Mod;

/// Curried arguments for the `everything_with_singleton` TAIL, which lets a singleton mint and
/// melt a CAT.
///
/// When the TAIL is run, it outputs a `RECEIVE_MESSAGE` condition (mode `0b010_111`, so the
/// sender is identified by its puzzle hash and the receiver by its coin id) whose message is the
/// amount being issued or melted (the CAT's delta). The sender must be the singleton with the
/// given launcher id, using the inner puzzle hash from the solution.
///
/// ## Example
///
/// ```rust
/// # use chia_protocol::{Bytes, Bytes32};
/// # use chia_sdk_types::{Mod, puzzles::EverythingWithSingletonTailArgs};
/// let launcher_id = Bytes32::new([1; 32]);
///
/// // The nonce lets the same singleton control more than one CAT.
/// let first = EverythingWithSingletonTailArgs::new(launcher_id, Bytes::default());
/// let second = EverythingWithSingletonTailArgs::new(launcher_id, Bytes::from(vec![1]));
///
/// // The asset id of a CAT is the tree hash of its TAIL.
/// assert_ne!(first.curry_tree_hash(), second.curry_tree_hash());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct EverythingWithSingletonTailArgs {
    /// The mod hash of the singleton top layer puzzle.
    pub singleton_mod_hash: Bytes32,
    /// The tree hash of the singleton struct, `(SINGLETON_MOD_HASH . (LAUNCHER_ID .
    /// LAUNCHER_PUZZLE_HASH))`.
    pub singleton_struct_hash: Bytes32,
    /// An arbitrary value that makes the TAIL (and therefore the asset id) unique.
    pub nonce: Bytes,
}

impl EverythingWithSingletonTailArgs {
    /// Creates new [`EverythingWithSingletonTailArgs`] for the singleton with the given launcher
    /// id, using the standard singleton top layer and launcher.
    pub fn new(launcher_id: Bytes32, nonce: Bytes) -> Self {
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

/// The solution for the `everything_with_singleton` TAIL. See [`EverythingWithSingletonTailArgs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct EverythingWithSingletonTailSolution {
    /// The current inner puzzle hash of the singleton, used to compute its full puzzle hash.
    pub singleton_inner_puzzle_hash: Bytes32,
}

impl EverythingWithSingletonTailSolution {
    /// Creates a new [`EverythingWithSingletonTailSolution`] from the singleton's inner puzzle
    /// hash.
    pub fn new(singleton_inner_puzzle_hash: Bytes32) -> Self {
        Self {
            singleton_inner_puzzle_hash,
        }
    }
}
