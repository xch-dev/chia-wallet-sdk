use std::borrow::Cow;

use chia_puzzles::{DELEGATED_PUZZLE_FEEDER, DELEGATED_PUZZLE_FEEDER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct DelegatedPuzzleFeederArgs<I> {
    pub inner_puzzle: I,
}

impl<I> DelegatedPuzzleFeederArgs<I> {
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

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct DelegatedPuzzleFeederSolution<P, S, I> {
    pub delegated_puzzle: P,
    pub delegated_solution: S,
    #[clvm(rest)]
    pub inner_solution: I,
}

impl<P, S, I> DelegatedPuzzleFeederSolution<P, S, I> {
    pub fn new(delegated_puzzle: P, delegated_solution: S, inner_solution: I) -> Self {
        Self {
            delegated_puzzle,
            delegated_solution,
            inner_solution,
        }
    }
}
