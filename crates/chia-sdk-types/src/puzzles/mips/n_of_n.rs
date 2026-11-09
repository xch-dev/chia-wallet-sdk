use std::borrow::Cow;

use chia_puzzles::{N_OF_N, N_OF_N_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS `n_of_n` puzzle, which requires every member to approve a
/// delegated puzzle hash.
///
/// Each member is run with the delegated puzzle hash prepended to its solution, and their
/// conditions are combined. There must be at least one member.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct NofNArgs<T> {
    /// The member puzzles, all of which are run.
    pub members: Vec<T>,
}

impl<T> NofNArgs<T> {
    /// Creates new [`NofNArgs`] from the list of members.
    pub fn new(members: Vec<T>) -> Self {
        Self { members }
    }
}

impl<T> Mod for NofNArgs<T> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&N_OF_N)
    }

    fn mod_hash() -> TreeHash {
        N_OF_N_HASH.into()
    }
}

/// The solution for the MIPS `n_of_n` puzzle. See [`NofNArgs`].
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NofNSolution<T> {
    /// The solution for each member, in the same order as the members.
    pub member_solutions: Vec<T>,
}

impl<T> NofNSolution<T> {
    /// Creates a new [`NofNSolution`] from the member solutions.
    pub fn new(member_solutions: Vec<T>) -> Self {
        Self { member_solutions }
    }
}
