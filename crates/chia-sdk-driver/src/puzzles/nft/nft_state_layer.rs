use chia_puzzles::nft::NftStateLayerArgs;
use chia_puzzles::nft::NFT_STATE_LAYER_PUZZLE_HASH;
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::FromSpend;

#[derive(Debug, Clone, Copy)]
pub struct NftStateLayerInfo<I = NodePtr, M = NodePtr> {
    pub inner_puzzle: I,
    pub metadata: M,
}

// impl<I, M> FromSpend<()> NftStateLayerInfo<I, M>
// where
//     I: FromClvm<NodePtr>,
//     M: FromClvm<NodePtr>,
// {
//     fn from_spend(
//         allocator: &mut Allocator,
//         cs: &CoinSpend,
//         prev_state_info: N,
//     ) -> Result<(), FromSpendError> {

//     }
//     pub fn parse(allocator: &Allocator, puzzle: &Puzzle) -> Result<Option<Self>, ParseError> {
//         let Some(puzzle) = puzzle.as_curried() else {
//             return Ok(None);
//         };

//         if puzzle.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH {
//             return Ok(None);
//         }

//         let state_args = NftStateLayerArgs::from_clvm(allocator, puzzle.args)?;

//         if state_args.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH.into() {
//             return Err(ParseError::InvalidModHash);
//         }

//         Ok(Some(Self {
//             inner_puzzle: state_args.inner_puzzle,
//             metadata: state_args.metadata,
//         }))
//     }
// }
