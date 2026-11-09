use std::borrow::Cow;

use chia_puzzles::{DELEGATED_PUZZLE_FEEDER, DELEGATED_PUZZLE_FEEDER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS delegated puzzle feeder, which is the outermost layer of a MIPS
/// inner puzzle.
///
/// The solution contains a delegated puzzle (which outputs the conditions the spender wants) and
/// the inner solution. The feeder runs the inner puzzle with the tree hash of the delegated puzzle
/// prepended to the inner solution, so that the members can approve that exact hash. It then
/// outputs the inner puzzle's conditions followed by the delegated puzzle's conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct DelegatedPuzzleFeederArgs<I> {
    /// The inner puzzle (such as a member, `m_of_n`, or restrictions), which approves the
    /// delegated puzzle hash.
    pub inner_puzzle: I,
}

impl<I> DelegatedPuzzleFeederArgs<I> {
    /// Creates new [`DelegatedPuzzleFeederArgs`] from the inner puzzle.
    pub fn new(inner_puzzle: I) -> Self {
        Self { inner_puzzle }
    }
}

impl<I> Mod for DelegatedPuzzleFeederArgs<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&DELEGATED_PUZZLE_FEEDER)
    }

    fn mod_hash() -> TreeHash {
        DELEGATED_PUZZLE_FEEDER_HASH.into()
    }
}

/// The solution for the MIPS delegated puzzle feeder. See [`DelegatedPuzzleFeederArgs`].
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct DelegatedPuzzleFeederSolution<P, S, I> {
    /// The delegated puzzle, whose tree hash is passed to the inner puzzle.
    pub delegated_puzzle: P,
    /// The solution for the delegated puzzle.
    pub delegated_solution: S,
    /// The solution for the inner puzzle, which comes after the delegated puzzle hash.
    #[clvm(rest)]
    pub inner_solution: I,
}

impl<P, S, I> DelegatedPuzzleFeederSolution<P, S, I> {
    /// Creates a new [`DelegatedPuzzleFeederSolution`] from the delegated puzzle, its solution, and
    /// the inner solution.
    pub fn new(delegated_puzzle: P, delegated_solution: S, inner_solution: I) -> Self {
        Self {
            delegated_puzzle,
            delegated_solution,
            inner_solution,
        }
    }
}
