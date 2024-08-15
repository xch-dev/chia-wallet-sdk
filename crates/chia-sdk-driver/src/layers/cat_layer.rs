use chia_protocol::Bytes32;
use chia_puzzles::cat::{CatArgs, CatSolution, CAT_PUZZLE_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug)]
pub struct CatLayer<IP> {
    pub asset_id: Bytes32,
    pub inner_puzzle: IP,
}

impl<IP> Layer for CatLayer<IP>
where
    IP: Layer,
{
    type Solution = CatSolution<IP::Solution>;

    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let parent_puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(parent_puzzle) = parent_puzzle.as_curried() else {
            return Ok(None);
        };

        if parent_puzzle.mod_hash != CAT_PUZZLE_HASH {
            return Ok(None);
        }

        let parent_args = CatArgs::<NodePtr>::from_clvm(allocator, parent_puzzle.args)?;

        if parent_args.mod_hash != CAT_PUZZLE_HASH.into() {
            return Err(DriverError::InvalidModHash);
        }

        let parent_sol = CatSolution::<NodePtr>::from_clvm(allocator, layer_solution)?;

        match IP::from_parent_spend(
            allocator,
            parent_args.inner_puzzle,
            parent_sol.inner_puzzle_solution,
        )? {
            None => Ok(None),
            Some(inner_puzzle) => Ok(Some(CatLayer::<IP> {
                asset_id: parent_args.asset_id,
                inner_puzzle,
            })),
        }
    }

    fn from_puzzle(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != CAT_PUZZLE_HASH {
            return Ok(None);
        }

        let args = CatArgs::<NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.mod_hash != CAT_PUZZLE_HASH.into() {
            return Err(DriverError::InvalidModHash);
        }

        match IP::from_puzzle(allocator, args.inner_puzzle)? {
            None => Ok(None),
            Some(inner_puzzle) => Ok(Some(CatLayer::<IP> {
                asset_id: args.asset_id,
                inner_puzzle,
            })),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.cat_puzzle().map_err(DriverError::Spend)?,
            args: CatArgs {
                mod_hash: CAT_PUZZLE_HASH.into(),
                asset_id: self.asset_id,
                inner_puzzle: self.inner_puzzle.construct_puzzle(ctx)?,
            },
        }
        .to_clvm(ctx.allocator_mut())
        .map_err(DriverError::ToClvm)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        CatSolution {
            inner_puzzle_solution: self
                .inner_puzzle
                .construct_solution(ctx, solution.inner_puzzle_solution)?,
            lineage_proof: solution.lineage_proof,
            prev_coin_id: solution.prev_coin_id,
            this_coin_info: solution.this_coin_info,
            next_coin_proof: solution.next_coin_proof,
            prev_subtotal: solution.prev_subtotal,
            extra_delta: solution.extra_delta,
        }
        .to_clvm(ctx.allocator_mut())
        .map_err(DriverError::ToClvm)
    }
}

impl<IP> ToTreeHash for CatLayer<IP>
where
    IP: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        CatArgs::curry_tree_hash(self.asset_id, self.inner_puzzle.tree_hash())
    }
}
