use chia_puzzles::{AUGMENTED_CONDITION, AUGMENTED_CONDITION_HASH};
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
    const MOD_REVEAL: &[u8] = &AUGMENTED_CONDITION;
    const MOD_HASH: TreeHash = TreeHash::new(AUGMENTED_CONDITION_HASH);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct AugmentedConditionSolution<I> {
    pub inner_solution: I,
}

impl<I> AugmentedConditionSolution<I> {
    pub fn new(inner_solution: I) -> Self {
        Self { inner_solution }
    }
}
