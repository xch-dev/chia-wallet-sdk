use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{Condition, Mod};

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct WrapConditionArgs<T, I> {
    pub condition: Condition<T>,
    pub inner_puzzle: I,
}

impl<T, I> WrapConditionArgs<T, I> {
    pub fn new(condition: Condition<T>, inner_puzzle: I) -> Self {
        Self {
            condition,
            inner_puzzle,
        }
    }
}

impl<T, I> Mod for WrapConditionArgs<T, I> {
    const MOD_REVEAL: &[u8] = &WRAP_CONDITION;
    const MOD_HASH: TreeHash = WRAP_CONDITION_HASH;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct WrapConditionSolution<I> {
    pub inner_solution: I,
}

impl<I> WrapConditionSolution<I> {
    pub fn new(inner_solution: I) -> Self {
        Self { inner_solution }
    }
}

/// ```lisp
/// (mod (CONDITION PUZZLE . solution)
///     (c CONDITION (a PUZZLE solution))
/// )```
pub const WRAP_CONDITION: [u8; 13] = hex!("ff04ff02ffff02ff05ff078080");

pub const WRAP_CONDITION_HASH: TreeHash = TreeHash::new(hex!(
    "4490c0eaf285dea913d4e3449894ab3c5f9beae58cfcf388f9536ab38212bf4a"
));

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(WRAP_CONDITION => WRAP_CONDITION_HASH);
        Ok(())
    }
}
