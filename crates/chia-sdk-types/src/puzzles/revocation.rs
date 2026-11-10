use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{REVOCATION_LAYER, REVOCATION_LAYER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the revocation layer, which lets a hidden puzzle spend a coin without the
/// inner puzzle's permission.
///
/// The coin can be spent either by the inner puzzle or by the hidden puzzle, depending on
/// [`RevocationSolution::hidden`]. When the inner puzzle is used, every `CREATE_COIN` it outputs
/// is wrapped in the revocation layer again with the same hidden puzzle hash. When the hidden
/// puzzle is used, its output isn't changed. This is used by revocable CATs and verifiable
/// credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct RevocationArgs {
    /// The mod hash of the revocation layer.
    pub mod_hash: Bytes32,
    /// The tree hash of the hidden puzzle, which can spend the coin at any time.
    pub hidden_puzzle_hash: Bytes32,
    /// The tree hash of the inner puzzle, which is revealed in the solution.
    pub inner_puzzle_hash: Bytes32,
}

impl RevocationArgs {
    /// Creates new [`RevocationArgs`], using the standard revocation layer mod hash.
    pub fn new(hidden_puzzle_hash: Bytes32, inner_puzzle_hash: Bytes32) -> Self {
        Self {
            mod_hash: REVOCATION_LAYER_HASH.into(),
            hidden_puzzle_hash,
            inner_puzzle_hash,
        }
    }
}

impl Mod for RevocationArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REVOCATION_LAYER)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(REVOCATION_LAYER_HASH)
    }
}

/// The solution for the revocation layer. See [`RevocationArgs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RevocationSolution<P, S> {
    /// Whether to run the hidden puzzle instead of the inner puzzle.
    pub hidden: bool,
    /// The hidden or inner puzzle, whose tree hash must match the curried hash.
    pub puzzle: P,
    /// The solution to run the puzzle with.
    pub solution: S,
}

impl<P, S> RevocationSolution<P, S> {
    /// Creates a new [`RevocationSolution`] from whether the hidden puzzle is used, the puzzle,
    /// and its solution.
    pub fn new(hidden: bool, puzzle: P, solution: S) -> Self {
        Self {
            hidden,
            puzzle,
            solution,
        }
    }
}
