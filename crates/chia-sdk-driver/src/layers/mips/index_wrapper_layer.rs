use chia_sdk_types::puzzles::{INDEX_WRAPPER_HASH, IndexWrapperArgs};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexWrapperLayer<N, I> {
    pub nonce: N,
    pub inner_puzzle: I,
}

impl<N, I> IndexWrapperLayer<N, I> {
    pub fn new(nonce: N, inner_puzzle: I) -> Self {
        Self {
            nonce,
            inner_puzzle,
        }
    }
}

impl<N, I> Layer for IndexWrapperLayer<N, I>
where
    N: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    I: Layer,
{
    type Solution = I::Solution;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        ctx.curry(IndexWrapperArgs::new(self.nonce.clone(), inner_puzzle))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        self.inner_puzzle.construct_solution(ctx, solution)
    }

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(curried) = puzzle.as_curried() else {
            return Ok(None);
        };

        if curried.mod_hash != INDEX_WRAPPER_HASH {
            return Ok(None);
        }

        let args = IndexWrapperArgs::<N, Puzzle>::from_clvm(allocator, curried.args)?;

        let Some(inner_puzzle) = I::parse_puzzle(allocator, args.inner_puzzle)? else {
            return Ok(None);
        };

        Ok(Some(Self::new(args.nonce, inner_puzzle)))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        I::parse_solution(allocator, solution)
    }
}
