use crate::{ParseError, Puzzle};
use chia_puzzles::nft::NftStateLayerArgs;
use chia_puzzles::nft::NFT_STATE_LAYER_PUZZLE_HASH;
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

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
