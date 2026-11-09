mod enforce_delegated_puzzle_wrappers;
mod force_1_of_2_restricted_variable;

use std::borrow::Cow;

use chia_puzzles::{RESTRICTIONS, RESTRICTIONS_HASH};
pub use enforce_delegated_puzzle_wrappers::*;
pub use force_1_of_2_restricted_variable::*;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS restrictions puzzle, which limits what an inner puzzle (usually
/// a member) can approve.
///
/// When run, it runs the inner puzzle with the delegated puzzle hash prepended to the inner
/// solution. Each member validator is run with `(conditions . solution)`, where `conditions` is
/// the inner puzzle's output, and each delegated puzzle validator is run with
/// `(delegated_puzzle_hash . solution)`. The validators can fail to reject the spend, but their
/// output is ignored, and the inner puzzle's conditions are returned unchanged.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct RestrictionsArgs<MV, DV, I> {
    /// The list of programs that check the conditions output by the inner puzzle, such as
    /// [`Timelock`](super::Timelock).
    pub member_validators: MV,
    /// The list of programs that check the delegated puzzle hash, such as
    /// [`EnforceDelegatedPuzzleWrappers`].
    pub delegated_puzzle_validators: DV,
    /// The inner puzzle being restricted.
    pub inner_puzzle: I,
}

impl<MV, DV, I> RestrictionsArgs<MV, DV, I> {
    /// Creates new [`RestrictionsArgs`] from the validators and inner puzzle.
    pub fn new(member_validators: MV, delegated_puzzle_validators: DV, inner_puzzle: I) -> Self {
        Self {
            member_validators,
            delegated_puzzle_validators,
            inner_puzzle,
        }
    }
}

impl<MV, DV, I> Mod for RestrictionsArgs<MV, DV, I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&RESTRICTIONS)
    }

    fn mod_hash() -> TreeHash {
        RESTRICTIONS_HASH.into()
    }
}

/// The solution for the MIPS restrictions puzzle. See [`RestrictionsArgs`].
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RestrictionsSolution<MV, DV, I> {
    /// The solution for each member validator, in the same order as the validators.
    pub member_validator_solutions: Vec<MV>,
    /// The solution for each delegated puzzle validator, in the same order as the validators.
    pub delegated_puzzle_validator_solutions: Vec<DV>,
    /// The solution for the inner puzzle, which comes after the delegated puzzle hash.
    pub inner_solution: I,
}

impl<MV, DV, I> RestrictionsSolution<MV, DV, I> {
    /// Creates a new [`RestrictionsSolution`] from the validator solutions and inner solution.
    pub fn new(
        member_validator_solutions: Vec<MV>,
        delegated_puzzle_validator_solutions: Vec<DV>,
        inner_solution: I,
    ) -> Self {
        Self {
            member_validator_solutions,
            delegated_puzzle_validator_solutions,
            inner_solution,
        }
    }
}
