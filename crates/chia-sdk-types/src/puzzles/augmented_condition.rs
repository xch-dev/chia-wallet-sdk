use chia_puzzles::{AUGMENTED_CONDITION, AUGMENTED_CONDITION_HASH};

use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::{Condition, Mod};

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AugmentedConditionArgs<T, I> {
    pub condition: Condition<T>,
    pub inner_puzzle: I,
}

impl<T, I> AugmentedConditionArgs<T, I> {
    pub fn new(condition: Condition<T>, inner_puzzle: I) -> Self {
        Self {
            condition,
            inner_puzzle,
        }
    }
}

impl<T, I> Mod for AugmentedConditionArgs<T, I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&AUGMENTED_CONDITION)
    }

    fn mod_hash() -> TreeHash {
        AUGMENTED_CONDITION_HASH.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AugmentedConditionSolution<I> {
    pub inner_solution: I,
}

impl<I> AugmentedConditionSolution<I> {
    pub fn new(inner_solution: I) -> Self {
        Self { inner_solution }
    }
}
