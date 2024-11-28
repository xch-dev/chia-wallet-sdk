use chia_sdk_types::Condition;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, TreeHash};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The augmented condition [`Layer`] allows for adding a condition to a puzzle's output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AugmentedConditionLayer<T, I> {
    pub condition: Condition<T>,
    pub inner_puzzle: I,
}

impl<T, I> Layer for AugmentedConditionLayer<T, I>
where
    T: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    I: Layer,
{
    type Solution = AugmentedConditionSolution<NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != AUGMENTED_CONDITION_PUZZLE_HASH {
            return Ok(None);
        }

        let args = AugmentedConditionArgs::<T, Puzzle>::from_clvm(allocator, puzzle.args)?;
        let Some(inner_layer) = I::parse_puzzle(allocator, args.inner_puzzle)? else {
            return Ok(None);
        };

        Ok(Some(Self {
            condition: args.condition,
            inner_puzzle: inner_layer,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(AugmentedConditionSolution::<NodePtr>::from_clvm(
            allocator, solution,
        )?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.augmented_condition_puzzle()?,
            args: AugmentedConditionArgs {
                condition: self.condition.clone(),
                inner_puzzle: self.inner_puzzle.construct_puzzle(ctx)?,
            },
        };
        ctx.alloc(&curried)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

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
