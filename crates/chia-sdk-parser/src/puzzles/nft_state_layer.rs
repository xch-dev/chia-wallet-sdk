use chia_protocol::Bytes32;
use chia_puzzles::nft::NFT_STATE_LAYER_PUZZLE_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{ParseError, Puzzle};

#[derive(Debug, Clone, Copy)]
pub struct NftStatePuzzle<I = Puzzle, M = NodePtr> {
    pub inner_puzzle: I,
    pub metadata: M,
}

impl<I, M> NftStatePuzzle<I, M>
where
    I: FromClvm<NodePtr>,
    M: FromClvm<NodePtr>,
{
    pub fn parse(allocator: &Allocator, puzzle: &Puzzle) -> Result<Option<Self>, ParseError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let state_args = NftStateLayerArgs::<I, M>::from_clvm(allocator, puzzle.args)?;

        if state_args.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH.into() {
            return Err(ParseError::InvalidModHash);
        }

        Ok(Some(Self {
            inner_puzzle: state_args.inner_puzzle,
            metadata: state_args.metadata,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[clvm(curry)]
pub struct NftStateLayerArgs<I, M> {
    pub mod_hash: Bytes32,
    pub metadata: M,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub inner_puzzle: I,
}

impl<I, M> NftStateLayerArgs<I, M> {
    pub fn new(metadata: M, inner_puzzle: I, metadata_updater_puzzle_hash: Bytes32) -> Self {
        Self {
            mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
            metadata,
            metadata_updater_puzzle_hash,
            inner_puzzle,
        }
    }
}

impl NftStateLayerArgs<TreeHash, TreeHash> {
    pub fn curry_tree_hash(
        metadata: TreeHash,
        inner_puzzle: TreeHash,
        metadata_updater_puzzle_hash: Bytes32,
    ) -> TreeHash {
        CurriedProgram {
            program: NFT_STATE_LAYER_PUZZLE_HASH,
            args: NftStateLayerArgs {
                mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                metadata,
                metadata_updater_puzzle_hash,
                inner_puzzle,
            },
        }
        .tree_hash()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[clvm(list)]
pub struct NftStateLayerSolution<I> {
    pub inner_solution: I,
}
