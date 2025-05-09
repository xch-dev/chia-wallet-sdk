use chia_puzzles::AUGMENTED_CONDITION_HASH;
use chia_sdk_types::{
    puzzles::{AugmentedConditionArgs, AugmentedConditionSolution},
    Condition,
};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

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

        if puzzle.mod_hash != AUGMENTED_CONDITION_HASH.into() {
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
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        let curried = ctx.curry(AugmentedConditionArgs {
            condition: self.condition.clone(),
            inner_puzzle,
        })?;
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
