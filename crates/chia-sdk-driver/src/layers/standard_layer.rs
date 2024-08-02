use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_puzzles::standard::{StandardArgs, StandardSolution, STANDARD_PUZZLE_HASH};
use chia_sdk_types::conditions::{run_puzzle, Condition, CreateCoin};
use clvm_traits::{FromClvm, ToClvm, ToNodePtr};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{Conditions, ParseError, Puzzle, PuzzleLayer, SpendContext};

// this is the innermost puzzle for most things
// we only need to know the synthetic key when spending
// this allows the lib to be used to e.g., parse NFT spends
// and get the latest coin's metadata
#[derive(Debug, Copy, Clone)]

pub struct StandardLayer {
    pub puzzle_hash: TreeHash,
    pub synthetic_key: Option<PublicKey>,
}

#[derive(Debug, ToClvm, FromClvm)]
#[clvm(list)]

pub struct StandardLayerSolution<T>
where
    T: FromClvm<NodePtr> + ToClvm<NodePtr>,
{
    #[clvm(rest)]
    pub conditions: Vec<Condition<T>>,
}

impl StandardLayer {
    pub fn new(synthetic_key: PublicKey) -> Self {
        StandardLayer {
            puzzle_hash: StandardArgs::curry_tree_hash(synthetic_key),
            synthetic_key: Some(synthetic_key),
        }
    }

    pub fn with_synthetic_key(mut self, synthetic_key: PublicKey) -> Self {
        self.synthetic_key = Some(synthetic_key);
        self
    }
}

impl PuzzleLayer<StandardLayerSolution<NodePtr>> for StandardLayer {
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

        return Ok(Some(StandardLayer {
            puzzle_hash: new_puzzle_hash.into(),
            synthetic_key: if StandardArgs::curry_tree_hash(parent_args.synthetic_key)
                == new_puzzle_hash.into()
            {
                Some(parent_args.synthetic_key)
            } else {
                None
            },
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
            puzzle_hash: puzzle.curried_puzzle_hash,
            synthetic_key: Some(args.synthetic_key),
        }))
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, ParseError> {
        CurriedProgram {
            program: ctx
                .standard_puzzle()
                .map_err(|err| ParseError::Spend(err))?,
            args: StandardArgs {
                synthetic_key: self.synthetic_key.ok_or(ParseError::MissingSyntheticKey)?,
            },
        }
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| ParseError::ToClvm(err))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: StandardLayerSolution<NodePtr>,
    ) -> Result<NodePtr, ParseError> {
        ctx.alloc(&StandardSolution::from_conditions(
            Conditions::new().conditions(&solution.conditions),
        ))
        .map_err(|err| ParseError::Spend(err))
    }
}

impl ToTreeHash for StandardLayer {
    fn tree_hash(&self) -> TreeHash {
        self.puzzle_hash
    }
}
