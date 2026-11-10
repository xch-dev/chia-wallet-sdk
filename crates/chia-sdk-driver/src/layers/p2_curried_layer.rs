use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::{P2_CURRIED_PUZZLE_HASH, P2CurriedArgs, P2CurriedSolution};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The p2 curried [`Layer`] commits to the hash of a puzzle, which is revealed when spending.
///
/// The solution contains the puzzle and its solution. The spend fails unless the tree hash of
/// the puzzle matches the curried puzzle hash, in which case the puzzle is run with the solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2CurriedLayer {
    /// The tree hash of the puzzle that must be revealed to spend the coin.
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
