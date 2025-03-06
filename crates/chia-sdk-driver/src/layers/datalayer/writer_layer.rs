use chia_puzzle_types::standard::StandardSolution;
use chia_sdk_types::{
    puzzles::{WriterLayerArgs, WriterLayerSolution, WRITER_LAYER_PUZZLE_HASH},
    Conditions,
};
use clvm_traits::{clvm_quote, FromClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext, StandardLayer};

/// The Writer [`Layer`] removes an authorized puzzle's ability to change the list of authorized puzzles.
/// It's typically used with [`DelegationLayer`](crate::DelegationLayer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterLayer<I> {
    /// The inner puzzle layer, commonly used for determining ownership.
    pub inner_puzzle: I,
}

impl<I> WriterLayer<I> {
    pub fn new(inner_puzzle: I) -> Self {
        Self { inner_puzzle }
    }
}

impl<I> Layer for WriterLayer<I>
where
    I: Layer,
{
    type Solution = I::Solution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != WRITER_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = WriterLayerArgs::<NodePtr>::from_clvm(allocator, puzzle.args)?;

        let Some(inner_puzzle) =
            I::parse_puzzle(allocator, Puzzle::parse(allocator, args.inner_puzzle))?
        else {
            return Ok(None);
        };

        Ok(Some(Self { inner_puzzle }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        let inner_solution =
            WriterLayerSolution::<NodePtr>::from_clvm(allocator, solution)?.inner_solution;

        I::parse_solution(allocator, inner_solution)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        let curried = ctx.curry(WriterLayerArgs::new(inner_puzzle))?;
        ctx.alloc(&curried)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let inner_solution = self.inner_puzzle.construct_solution(ctx, solution)?;

        ctx.alloc(&WriterLayerSolution::<NodePtr> { inner_solution })
    }
}

impl<I> ToTreeHash for WriterLayer<I>
where
    I: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        let inner_puzzle_hash = self.inner_puzzle.tree_hash();

        WriterLayerArgs::curry_tree_hash(inner_puzzle_hash)
    }
}

impl WriterLayer<StandardLayer> {
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        output_conditions: Conditions,
    ) -> Result<Spend, DriverError> {
        let dp = ctx.alloc(&clvm_quote!(output_conditions))?;
        let solution = self.construct_solution(
            ctx,
            StandardSolution {
                original_public_key: None,
                delegated_puzzle: dp,
                solution: NodePtr::NIL,
            },
        )?;
        let puzzle = self.construct_puzzle(ctx)?;

        Ok(Spend { puzzle, solution })
    }
}
