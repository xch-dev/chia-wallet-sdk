use chia_puzzles::DELEGATED_PUZZLE_FEEDER_HASH;
use chia_sdk_types::puzzles::{DelegatedPuzzleFeederArgs, DelegatedPuzzleFeederSolution};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelegatedPuzzleFeederLayer<I> {
    pub inner_puzzle: I,
}

impl<I> DelegatedPuzzleFeederLayer<I> {
    pub fn new(inner_puzzle: I) -> Self {
        Self { inner_puzzle }
    }
}

impl<I> Layer for DelegatedPuzzleFeederLayer<I>
where
    I: Layer,
{
    type Solution = DelegatedPuzzleFeederSolution<NodePtr, NodePtr, I::Solution>;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        ctx.curry(DelegatedPuzzleFeederArgs::new(inner_puzzle))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let inner_solution = self
            .inner_puzzle
            .construct_solution(ctx, solution.inner_solution)?;
        ctx.alloc(&DelegatedPuzzleFeederSolution::new(
            solution.delegated_puzzle,
            solution.delegated_solution,
            inner_solution,
        ))
    }

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(curried) = puzzle.as_curried() else {
            return Ok(None);
        };

        if curried.mod_hash != DELEGATED_PUZZLE_FEEDER_HASH.into() {
            return Ok(None);
        }

        let args = DelegatedPuzzleFeederArgs::<Puzzle>::from_clvm(allocator, curried.args)?;

        let Some(inner_puzzle) = I::parse_puzzle(allocator, args.inner_puzzle)? else {
            return Ok(None);
        };

        Ok(Some(Self::new(inner_puzzle)))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        let solution = DelegatedPuzzleFeederSolution::<NodePtr, NodePtr, NodePtr>::from_clvm(
            allocator, solution,
        )?;

        let inner_solution = I::parse_solution(allocator, solution.inner_solution)?;

        Ok(DelegatedPuzzleFeederSolution::new(
            solution.delegated_puzzle,
            solution.delegated_solution,
            inner_solution,
        ))
    }
}
