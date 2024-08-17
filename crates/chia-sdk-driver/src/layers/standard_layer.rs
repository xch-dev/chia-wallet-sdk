use chia_bls::PublicKey;
use chia_puzzles::standard::{StandardArgs, StandardSolution, STANDARD_PUZZLE_HASH};
use chia_sdk_types::Conditions;
use clvm_traits::{clvm_quote, FromClvm};
use clvm_utils::CurriedProgram;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StandardLayer {
    pub synthetic_key: PublicKey,
}

impl StandardLayer {
    pub fn new(synthetic_key: PublicKey) -> Self {
        Self { synthetic_key }
    }

    // This lint is ignored since semantically we want spends to consume Conditions.
    #[allow(clippy::needless_pass_by_value)]
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        conditions: Conditions,
    ) -> Result<Spend, DriverError> {
        let delegated_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
        self.construct_spend(
            ctx,
            StandardSolution {
                original_public_key: None,
                delegated_puzzle,
                solution: NodePtr::NIL,
            },
        )
    }
}

impl Layer for StandardLayer {
    type Solution = StandardSolution<NodePtr, NodePtr>;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.standard_puzzle()?,
            args: StandardArgs::new(self.synthetic_key),
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
