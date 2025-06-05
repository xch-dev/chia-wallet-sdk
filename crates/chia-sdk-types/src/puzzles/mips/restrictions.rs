mod enforce_delegated_puzzle_wrappers;
mod force_1_of_2_restricted_variable;

use std::borrow::Cow;

pub use enforce_delegated_puzzle_wrappers::*;
pub use force_1_of_2_restricted_variable::*;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

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
        Cow::Borrowed(&RESTRICTIONS_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        RESTRICTIONS_PUZZLE_HASH
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

pub const RESTRICTIONS_PUZZLE: [u8; 204] = hex!(
    "
    ff02ffff01ff02ff04ffff04ff02ffff04ff05ffff04ff5fffff04ffff02ff17
    ffff04ff2fff82017f8080ffff04ffff02ff06ffff04ff02ffff04ff0bffff04
    ff81bfffff04ff2fff808080808080ff80808080808080ffff04ffff01ffff03
    ff80ffff02ff06ffff04ff02ffff04ff05ffff04ff0bffff04ff17ff80808080
    8080ff1780ff02ffff03ff05ffff01ff03ff80ffff02ff09ffff04ff17ff1380
    80ffff02ff06ffff04ff02ffff04ff0dffff04ff1bffff04ff17ff8080808080
    8080ff8080ff0180ff018080
    "
);

pub const RESTRICTIONS_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "a28d59d39f964a93159c986b1914694f6f2f1c9901178f91e8b0ba4045980eef"
));
