use std::borrow::Cow;

use chia_puzzles::{ADD_DPUZ_WRAPPER, ADD_DPUZ_WRAPPER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS `add_dpuz_wrapper` puzzle, which wraps a delegated puzzle in a
/// restriction.
///
/// When run, it runs the delegated puzzle with the delegated solution, then runs the wrapper with
/// `(conditions . wrapper_solution)`, so that the wrapper can check or change the conditions. The
/// [`EnforceDelegatedPuzzleWrappers`](super::EnforceDelegatedPuzzleWrappers) restriction requires
/// the delegated puzzle of a MIPS spend to be wrapped this way.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AddDelegatedPuzzleWrapper<W, P> {
    /// The wrapper program, which receives the delegated puzzle's conditions.
    pub wrapper: W,
    /// The delegated puzzle that is being wrapped.
    pub delegated_puzzle: P,
}

impl<W, P> AddDelegatedPuzzleWrapper<W, P> {
    /// Creates a new [`AddDelegatedPuzzleWrapper`] from the wrapper and delegated puzzle.
    pub fn new(wrapper: W, delegated_puzzle: P) -> Self {
        Self {
            wrapper,
            delegated_puzzle,
        }
    }
}

impl<W, P> Mod for AddDelegatedPuzzleWrapper<W, P> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&ADD_DPUZ_WRAPPER)
    }

    fn mod_hash() -> TreeHash {
        ADD_DPUZ_WRAPPER_HASH.into()
    }
}

/// The solution for the MIPS `add_dpuz_wrapper` puzzle. See [`AddDelegatedPuzzleWrapper`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AddDelegatedPuzzleWrapperSolution<W, P> {
    /// The solution passed to the wrapper after the conditions.
    pub wrapper_solution: W,
    /// The solution for the delegated puzzle.
    pub delegated_solution: P,
}

impl<W, P> AddDelegatedPuzzleWrapperSolution<W, P> {
    /// Creates a new [`AddDelegatedPuzzleWrapperSolution`] from the wrapper and delegated
    /// solutions.
    pub fn new(wrapper_solution: W, delegated_solution: P) -> Self {
        Self {
            wrapper_solution,
            delegated_solution,
        }
    }
}
