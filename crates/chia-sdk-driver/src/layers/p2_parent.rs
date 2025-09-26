use chia_sdk_types::puzzles::{P2ParentArgs, P2ParentSolution, P2_PARENT_PUZZLE_HASH};
use clvm_traits::FromClvm;
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{CatMaker, DriverError, Layer, Puzzle, SpendContext};

/// The p2 parent [`Layer`] fixes a coin's delegated inner puzzle to match the parent's inner puzzle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2ParentLayer {
    pub morpher: CatMaker,
}

impl P2ParentLayer {
    pub fn xch() -> Self {
        Self {
            morpher: CatMaker::Xch,
        }
    }

    pub fn cat(tail_hash_hash: TreeHash) -> Self {
        Self {
            morpher: CatMaker::Default { tail_hash_hash },
        }
    }
}

impl Layer for P2ParentLayer {
    type Solution = P2ParentSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_PARENT_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2ParentArgs::from_clvm(allocator, puzzle.args)?;

        // todo: need CatMaker recognizer

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
