use chia_protocol::Bytes32;
use chia_sdk_types::conditions::{run_puzzle, CreateCoin};
use clvm_traits::FromClvm;
use clvm_utils::{tree_hash, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{ParseError, PuzzleLayer, SpendContext};

// this is the innermost puzzle for most things

#[derive(Debug, Copy, Clone)]

pub struct TransparentLayer {
    pub puzzle_hash: TreeHash,
    pub puzzle: Option<NodePtr>,
}

impl TransparentLayer {
    pub fn new(puzzle_hash: TreeHash, puzzle: Option<NodePtr>) -> Self {
        TransparentLayer {
            puzzle_hash,
            puzzle,
        }
    }

    pub fn from_puzzle(allocator: &Allocator, puzzle: NodePtr) -> Self {
        TransparentLayer {
            puzzle_hash: tree_hash(&allocator, puzzle),
            puzzle: Some(puzzle),
        }
    }

    pub fn with_puzzle(mut self, puzzle: NodePtr) -> Self {
        self.puzzle = Some(puzzle);
        self
    }
}

impl PuzzleLayer<NodePtr> for TransparentLayer {
    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        let output = run_puzzle(allocator, layer_puzzle, layer_solution)
            .map_err(|err| ParseError::Eval(err))?;
        let conditions = Vec::<NodePtr>::from_clvm(allocator, output)
            .map_err(|err| ParseError::FromClvm(err))?;

        // if there's only one output, we can predict this layer's puzzle hash
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

        if tree_hash(&allocator, layer_puzzle) == new_puzzle_hash.into() {
            return Ok(Some(TransparentLayer {
                puzzle_hash: new_puzzle_hash.into(),
                puzzle: Some(layer_puzzle),
            }));
        }

        Ok(Some(TransparentLayer {
            puzzle_hash: new_puzzle_hash.into(),
            puzzle: None,
        }))
    }

    fn from_puzzle(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        Ok(Some(TransparentLayer {
            puzzle_hash: tree_hash(&allocator, layer_puzzle),
            puzzle: Some(layer_puzzle),
        }))
    }

    fn construct_puzzle(&self, _: &mut SpendContext) -> Result<NodePtr, ParseError> {
        self.puzzle.ok_or(ParseError::MissingPuzzle)
    }

    fn construct_solution(
        &self,
        _: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<NodePtr, ParseError> {
        Ok(solution)
    }
}

impl ToTreeHash for TransparentLayer {
    fn tree_hash(&self) -> TreeHash {
        self.puzzle_hash
    }
}
