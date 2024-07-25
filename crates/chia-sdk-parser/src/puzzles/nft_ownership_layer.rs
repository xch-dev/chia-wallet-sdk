use chia_protocol::Bytes32;
use chia_puzzles::nft::{
    NftOwnershipLayerArgs, NftRoyaltyTransferPuzzleArgs, NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
    NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{ParseError, Puzzle};

#[derive(Debug, Clone, Copy)]
pub struct NftOwnershipPuzzle<I = NodePtr> {
    pub inner_puzzle: I,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_percentage: u16,
}

impl<I> NftOwnershipPuzzle<I>
where
    I: FromClvm<NodePtr>,
{
    pub fn parse(allocator: &Allocator, puzzle: &Puzzle) -> Result<Option<Self>, ParseError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        let ownership_args =
            NftOwnershipLayerArgs::<I, NodePtr>::from_clvm(allocator, puzzle.args)?;

        if ownership_args.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into() {
            return Err(ParseError::NonStandardLayer);
        }

        let Some(transfer_puzzle) =
            Puzzle::parse(allocator, ownership_args.transfer_program).as_curried()
        else {
            return Err(ParseError::NonStandardLayer);
        };

        if transfer_puzzle.mod_hash != NFT_ROYALTY_TRANSFER_PUZZLE_HASH {
            return Err(ParseError::NonStandardLayer);
        }

        let transfer_args =
            NftRoyaltyTransferPuzzleArgs::from_clvm(allocator, transfer_puzzle.args)?;

        Ok(Some(Self {
            inner_puzzle: ownership_args.inner_puzzle,
            current_owner: ownership_args.current_owner,
            royalty_puzzle_hash: transfer_args.royalty_puzzle_hash,
            royalty_percentage: transfer_args.trade_price_percentage,
        }))
    }
}
