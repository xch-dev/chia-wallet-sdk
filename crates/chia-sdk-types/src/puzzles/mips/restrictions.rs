mod enforce_delegated_puzzle_wrappers;
mod force_1_of_2_restricted_variable;

use std::borrow::Cow;

use chia_puzzles::{RESTRICTIONS, RESTRICTIONS_HASH};
pub use enforce_delegated_puzzle_wrappers::*;
pub use force_1_of_2_restricted_variable::*;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct RestrictionsArgs<MV, DV, I> {
    pub member_validators: MV,
    pub delegated_puzzle_validators: DV,
    pub inner_puzzle: I,
}

impl<MV, DV, I> RestrictionsArgs<MV, DV, I> {
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

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RestrictionsSolution<MV, DV, I> {
    pub member_validator_solutions: Vec<MV>,
    pub delegated_puzzle_validator_solutions: Vec<DV>,
    pub inner_solution: I,
}

impl<MV, DV, I> RestrictionsSolution<MV, DV, I> {
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
