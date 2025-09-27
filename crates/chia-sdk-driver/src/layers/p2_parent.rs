use chia_sdk_types::puzzles::{P2ParentArgs, P2ParentSolution, P2_PARENT_PUZZLE_HASH};
use clvm_traits::FromClvm;
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{CatMaker, DriverError, Layer, Puzzle, SpendContext};

/// The p2 parent [`Layer`] fixes a coin's delegated inner puzzle to match the parent's inner puzzle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2ParentLayer {
    pub cat_maker: CatMaker,
}

impl P2ParentLayer {
    pub fn xch() -> Self {
        Self {
            cat_maker: CatMaker::Xch,
        }
    }

    pub fn cat(tail_hash_hash: TreeHash) -> Self {
        Self {
            cat_maker: CatMaker::Default { tail_hash_hash },
        }
    }

    pub fn revocable_cat(tail_hash_hash: TreeHash, hidden_puzzle_hash_hash: TreeHash) -> Self {
        Self {
            cat_maker: CatMaker::Revocable {
                tail_hash_hash,
                hidden_puzzle_hash_hash,
            },
        }
    }
}

impl Layer for P2ParentLayer {
    type Solution = P2ParentSolution<NodePtr, NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_PARENT_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2ParentArgs::from_clvm(allocator, puzzle.args)?;
        let Some(cat_maker) =
            CatMaker::parse_puzzle(allocator, Puzzle::parse(allocator, args.cat_maker))?
        else {
            return Ok(None);
        };

        Ok(Some(Self { cat_maker }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2ParentSolution::<NodePtr, NodePtr, NodePtr>::from_clvm(
            allocator, solution,
        )?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let cat_maker = self.cat_maker.get_puzzle(ctx)?;

        let curried = ctx.curry(P2ParentArgs { cat_maker })?;
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
