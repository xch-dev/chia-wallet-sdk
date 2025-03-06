use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::{P2CurriedArgs, P2CurriedSolution, P2_CURRIED_PUZZLE_HASH};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The p2 curried [`Layer`] allows for .
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2CurriedLayer {
    pub puzzle_hash: Bytes32,
}

impl Layer for P2CurriedLayer {
    type Solution = P2CurriedSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_CURRIED_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2CurriedArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            puzzle_hash: args.puzzle_hash,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2CurriedSolution::<NodePtr, NodePtr>::from_clvm(
            allocator, solution,
        )?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = ctx.curry(P2CurriedArgs {
            puzzle_hash: self.puzzle_hash,
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
