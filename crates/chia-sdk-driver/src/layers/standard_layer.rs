use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_puzzles::standard::{StandardArgs, StandardSolution, STANDARD_PUZZLE_HASH};
use chia_sdk_types::conditions::{run_puzzle, CreateCoin};
use clvm_traits::{FromClvm, ToNodePtr};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{Conditions, ParseError, Puzzle, PuzzleLayer, SpendContext};

#[derive(Debug, Copy, Clone)]

pub struct StandardLayer {
    pub synthetic_key: PublicKey,
}

impl PuzzleLayer<Conditions> for StandardLayer {
    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        let parent_puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(parent_puzzle) = parent_puzzle.as_curried() else {
            return Ok(None);
        };

        if parent_puzzle.mod_hash != STANDARD_PUZZLE_HASH {
            return Ok(None);
        }

        let parent_args = StandardArgs::from_clvm(allocator, parent_puzzle.args)
            .map_err(|err| ParseError::FromClvm(err))?;

        let output = run_puzzle(allocator, layer_puzzle, layer_solution)
            .map_err(|err| ParseError::Eval(err))?;
        let conditions = Vec::<NodePtr>::from_clvm(allocator, output)
            .map_err(|err| ParseError::FromClvm(err))?;

        // if the puzzle hash matches and there's only one output, we can predict the child args from the parent
        let mut new_puzzle_hash: Option<Bytes32> = None;
        for condition in conditions {
            match CreateCoin::from_clvm(allocator, condition) {
                Ok(cc) => {
                    if new_puzzle_hash.is_some() {
                        return Ok(None);
                    }

                    new_puzzle_hash = Some(cc.puzzle_hash);
                }
                _ => {}
            }
        }

        let Some(new_puzzle_hash) = new_puzzle_hash else {
            return Ok(None);
        };
        if StandardArgs::curry_tree_hash(parent_args.synthetic_key) != new_puzzle_hash.into() {
            return Ok(None);
        }

        return Ok(Some(StandardLayer {
            synthetic_key: parent_args.synthetic_key,
        }));
    }

    fn from_puzzle(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        let puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != STANDARD_PUZZLE_HASH {
            return Ok(None);
        }

        let args = StandardArgs::from_clvm(allocator, puzzle.args)
            .map_err(|err| ParseError::FromClvm(err))?;

        Ok(Some(StandardLayer {
            synthetic_key: args.synthetic_key,
        }))
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, ParseError> {
        CurriedProgram {
            program: ctx
                .standard_puzzle()
                .map_err(|err| ParseError::Spend(err))?,
            args: StandardArgs {
                synthetic_key: self.synthetic_key,
            },
        }
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| ParseError::ToClvm(err))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Conditions,
    ) -> Result<NodePtr, ParseError> {
        ctx.alloc(&StandardSolution::from_conditions(solution))
            .map_err(|err| ParseError::Spend(err))
    }
}

impl ToTreeHash for StandardLayer {
    fn tree_hash(&self) -> TreeHash {
        StandardArgs::curry_tree_hash(self.synthetic_key)
    }
}
