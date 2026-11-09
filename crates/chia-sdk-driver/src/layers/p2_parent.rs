use chia_sdk_types::puzzles::{P2_PARENT_PUZZLE_HASH, P2ParentArgs, P2ParentSolution};
use clvm_traits::FromClvm;
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{CatMaker, DriverError, Layer, Puzzle, SpendContext};

/// The p2 parent [`Layer`] lets a coin be spent by the inner puzzle of its parent.
///
/// The solution reveals the parent's inner puzzle and a solution for it. The puzzle asserts that
/// its parent coin has that inner puzzle (wrapped by the [`CatMaker`] to get the full puzzle hash),
/// and outputs the conditions of the parent's inner puzzle. So whoever could spend the parent can
/// also spend this coin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2ParentLayer {
    /// The CAT maker that converts the parent's inner puzzle hash into its full puzzle hash.
    pub cat_maker: CatMaker,
}

impl P2ParentLayer {
    /// Creates a [`P2ParentLayer`] for a coin whose parent is an XCH coin.
    pub fn xch() -> Self {
        Self {
            cat_maker: CatMaker::Xch,
        }
    }

    /// Creates a [`P2ParentLayer`] for a coin whose parent is a CAT. The `tail_hash_hash` is the
    /// tree hash of the asset id, as in `asset_id.tree_hash()`.
    pub fn cat(tail_hash_hash: TreeHash) -> Self {
        Self {
            cat_maker: CatMaker::Default { tail_hash_hash },
        }
    }

    /// Creates a [`P2ParentLayer`] for a coin whose parent is a revocable CAT. The hashes are the
    /// tree hashes of the asset id and of the hidden puzzle hash.
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
