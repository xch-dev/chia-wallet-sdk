use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

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
    const MOD_REVEAL: &[u8] = &AUGMENTED_CONDITION_PUZZLE;
    const MOD_HASH: TreeHash = AUGMENTED_CONDITION_PUZZLE_HASH;
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

pub const AUGMENTED_CONDITION_PUZZLE: [u8; 13] = hex!("ff04ff02ffff02ff05ff0b8080");

pub const AUGMENTED_CONDITION_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "d303eafa617bedf0bc05850dd014e10fbddf622187dc07891a2aacba9d8a93f6"
));

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(AUGMENTED_CONDITION_PUZZLE => AUGMENTED_CONDITION_PUZZLE_HASH);
        Ok(())
    }
}
