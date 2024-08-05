use chia_protocol::Bytes32;
use chia_puzzles::cat::{CatArgs, CatSolution, CAT_PUZZLE_HASH};
use clvm_traits::{FromClvm, ToNodePtr};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{ParseError, Puzzle, PuzzleLayer, SpendContext};

#[derive(Debug)]

pub struct CATLayer<IP> {
    pub tail_hash: Bytes32,
    pub inner_puzzle: IP,
}

impl<IP> PuzzleLayer for CATLayer<IP>
where
    IP: PuzzleLayer,
{
    type Solution = CatSolution<IP::Solution>;

    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        todo!("todo");
    }

    fn from_puzzle(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        let puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != CAT_PUZZLE_HASH {
            return Ok(None);
        }

        let args = CatArgs::<NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.mod_hash != CAT_PUZZLE_HASH.into() {
            return Err(ParseError::InvalidModHash);
        }

        match IP::from_puzzle(allocator, args.inner_puzzle)? {
            None => return Ok(None),
            Some(inner_puzzle) => Ok(Some(CATLayer::<IP> {
                tail_hash: args.asset_id,
                inner_puzzle,
            })),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, ParseError> {
        CurriedProgram {
            program: ctx.cat_puzzle().map_err(|err| ParseError::Spend(err))?,
            args: CatArgs {
                mod_hash: CAT_PUZZLE_HASH.into(),
                asset_id: self.tail_hash,
                inner_puzzle: self.inner_puzzle.construct_puzzle(ctx)?,
            },
        }
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| ParseError::ToClvm(err))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, ParseError> {
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
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| ParseError::ToClvm(err))
    }
}

impl<IP> ToTreeHash for CATLayer<IP>
where
    IP: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        CatArgs::curry_tree_hash(self.tail_hash, self.inner_puzzle.tree_hash())
    }
}
