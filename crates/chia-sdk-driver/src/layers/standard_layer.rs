use chia_bls::PublicKey;
use chia_puzzles::standard::{StandardArgs, StandardSolution, STANDARD_PUZZLE_HASH};
use clvm_traits::FromClvm;
use clvm_utils::CurriedProgram;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StandardLayer {
    pub synthetic_key: PublicKey,
}

impl Layer for StandardLayer {
    type Solution = StandardSolution<NodePtr, NodePtr>;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.standard_puzzle()?,
            args: StandardArgs::new(self.synthetic_key),
        };
        Ok(ctx.alloc(&curried)?)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        Ok(ctx.alloc(&solution)?)
    }

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != STANDARD_PUZZLE_HASH {
            return Ok(None);
        }

        let args = StandardArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            synthetic_key: args.synthetic_key,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(StandardSolution::from_clvm(allocator, solution)?)
    }
}
