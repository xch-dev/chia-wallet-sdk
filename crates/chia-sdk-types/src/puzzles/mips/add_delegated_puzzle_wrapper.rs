use std::borrow::Cow;

use chia_puzzles::{ADD_DPUZ_WRAPPER, ADD_DPUZ_WRAPPER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AddDelegatedPuzzleWrapper<W, P> {
    pub wrapper: W,
    pub delegated_puzzle: P,
}

impl<W, P> AddDelegatedPuzzleWrapper<W, P> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AddDelegatedPuzzleWrapperSolution<W, P> {
    pub wrapper_solution: W,
    pub delegated_solution: P,
}

impl<W, P> AddDelegatedPuzzleWrapperSolution<W, P> {
    pub fn new(wrapper_solution: W, delegated_solution: P) -> Self {
        Self {
            wrapper_solution,
            delegated_solution,
        }
    }
}
