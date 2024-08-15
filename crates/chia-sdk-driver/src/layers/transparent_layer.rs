use chia_protocol::Bytes32;
use chia_sdk_types::conditions::{run_puzzle, CreateCoin};
use clvm_traits::FromClvm;
use clvm_utils::{tree_hash, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, SpendContext};

// this is the innermost puzzle for most things

// HINT_OVERRIDE: if true, the puzzle hash will be the hint, not the actual puzzle hash
//  of the found CREATE_COIN condition
// typically, you'd only set this to true if the upper layer doesn't wrap CREATE_COINs

/// A transparent layer makes
#[derive(Debug, Copy, Clone)]
pub struct TransparentLayer<const USE_HINT: bool = false> {
    pub puzzle_hash: TreeHash,
    pub puzzle: Option<NodePtr>,
}

impl<const HINT_OVERRIDE: bool> TransparentLayer<HINT_OVERRIDE> {
    pub fn new(puzzle_hash: TreeHash, puzzle: Option<NodePtr>) -> Self {
        TransparentLayer {
            puzzle_hash,
            puzzle,
        }
    }

    pub fn from_puzzle(allocator: &Allocator, puzzle: NodePtr) -> Self {
        TransparentLayer {
            puzzle_hash: tree_hash(allocator, puzzle),
            puzzle: Some(puzzle),
        }
    }

    #[must_use]
    pub fn with_puzzle(mut self, puzzle: NodePtr) -> Self {
        self.puzzle = Some(puzzle);
        self
    }
}

impl<const HINT_OVERRIDE: bool> Layer for TransparentLayer<HINT_OVERRIDE> {
    type Solution = NodePtr;

    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let output =
            run_puzzle(allocator, layer_puzzle, layer_solution).map_err(DriverError::Eval)?;
        let conditions =
            Vec::<NodePtr>::from_clvm(allocator, output).map_err(DriverError::FromClvm)?;

        // if there's only one output, we can predict this layer's puzzle hash
        let mut new_puzzle_hash: Option<Bytes32> = None;
        for condition in conditions {
            if let Ok(cc) = CreateCoin::from_clvm(allocator, condition) {
                if new_puzzle_hash.is_some() {
                    return Ok(None);
                }

                if HINT_OVERRIDE && cc.amount == 0 {
                    // e.g., DID created NFT
                    continue;
                }
                new_puzzle_hash = Some(
                    if HINT_OVERRIDE && !cc.memos.is_empty() && cc.memos[0].len() == 32 {
                        // standard puzzle will hint the inner puzzle hash
                        // this is useful e.g., when re-creatign a DID (created puz hash != actual transparent layer puz hash)
                        Bytes32::new(cc.memos[0].to_vec().try_into().unwrap())
                    } else {
                        cc.puzzle_hash
                    },
                );
            }
        }

        let Some(new_puzzle_hash) = new_puzzle_hash else {
            return Ok(None);
        };

        if tree_hash(allocator, layer_puzzle) == new_puzzle_hash.into() {
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
    ) -> Result<Option<Self>, DriverError> {
        Ok(Some(TransparentLayer {
            puzzle_hash: tree_hash(allocator, layer_puzzle),
            puzzle: Some(layer_puzzle),
        }))
    }

    fn construct_puzzle(&self, _: &mut SpendContext) -> Result<NodePtr, DriverError> {
        self.puzzle.ok_or(DriverError::MissingPuzzle)
    }
}

impl<const HINT_OVERRIDE: bool> ToTreeHash for TransparentLayer<HINT_OVERRIDE> {
    fn tree_hash(&self) -> TreeHash {
        self.puzzle_hash
    }
}
