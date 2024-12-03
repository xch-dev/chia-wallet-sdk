use chia_protocol::Bytes32;
use chia_sdk_types::{P2OneOfManyArgs, P2OneOfManySolution, P2_ONE_OF_MANY_PUZZLE_HASH};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The p2 1 of n [`Layer`] allows for picking from several delegated puzzles at runtime without revealing up front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2OneOfMany {
    /// The merkle root used to lookup the delegated puzzle as part of the solution.
    pub merkle_root: Bytes32,
}

impl Layer for P2OneOfMany {
    type Solution = P2OneOfManySolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_ONE_OF_MANY_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2OneOfManyArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            merkle_root: args.merkle_root,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2OneOfManySolution::<NodePtr, NodePtr>::from_clvm(
            allocator, solution,
        )?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2OneOfManyArgs {
            merkle_root: self.merkle_root,
        })
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}
