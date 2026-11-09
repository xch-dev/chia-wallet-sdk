use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{
    P2_SINGLETON, P2_SINGLETON_HASH, SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};

use crate::Mod;

/// Curried arguments for the p2 singleton puzzle, which lets a coin be spent by a singleton.
///
/// To spend the coin, the singleton must create a puzzle announcement of this coin's id. The
/// puzzle asserts that announcement from the singleton's full puzzle hash, asserts its own coin
/// id, and creates a coin announcement of `$` that the singleton can assert in return.
///
/// ## Example
///
/// ```rust
/// # use chia_protocol::Bytes32;
/// # use chia_sdk_types::{Mod, puzzles::P2SingletonArgs};
/// let launcher_id = Bytes32::new([1; 32]);
/// assert_eq!(
///     P2SingletonArgs::curry_tree_hash(launcher_id),
///     Mod::curry_tree_hash(&P2SingletonArgs::new(launcher_id))
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2SingletonArgs {
    /// The mod hash of the singleton top layer puzzle.
    pub singleton_mod_hash: Bytes32,
    /// The launcher id of the singleton that can spend the coin.
    pub launcher_id: Bytes32,
    /// The puzzle hash of the singleton launcher.
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
    /// Creates new [`P2SingletonArgs`] for the singleton with the given launcher id, using the
    /// standard singleton top layer and launcher.
    pub fn new(launcher_id: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            launcher_id,
            launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
        }
    }

    /// Calculates the puzzle hash of the p2 singleton puzzle for the singleton with the given
    /// launcher id.
    pub fn curry_tree_hash(launcher_id: Bytes32) -> TreeHash {
        CurriedProgram {
            program: TreeHash::new(P2_SINGLETON_HASH),
            args: Self::new(launcher_id),
        }
        .tree_hash()
    }
}

/// The solution for the p2 singleton puzzle. See [`P2SingletonArgs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2SingletonSolution {
    /// The current inner puzzle hash of the singleton, used to compute its full puzzle hash.
    pub singleton_inner_puzzle_hash: Bytes32,
    /// The coin id of the coin being spent.
    pub my_id: Bytes32,
}
