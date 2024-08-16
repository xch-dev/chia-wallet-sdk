use std::convert::Infallible;

use chia_protocol::Bytes32;
use chia_puzzles::{
    nft::{NftRoyaltyTransferPuzzleArgs, NFT_ROYALTY_TRANSFER_PUZZLE_HASH},
    singleton::SingletonStruct,
};
use clvm_traits::FromClvm;
use clvm_utils::CurriedProgram;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoyaltyTransferLayer {
    pub singleton_struct: SingletonStruct,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,
}

impl Layer for RoyaltyTransferLayer {
    type Solution = Infallible;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.nft_royalty_transfer()?,
            args: NftRoyaltyTransferPuzzleArgs {
                singleton_struct: self.singleton_struct,
                royalty_puzzle_hash: self.royalty_puzzle_hash,
                royalty_ten_thousandths: self.royalty_ten_thousandths,
            },
        };
        Ok(ctx.alloc(&curried)?)
    }

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != NFT_ROYALTY_TRANSFER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = NftRoyaltyTransferPuzzleArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            singleton_struct: args.singleton_struct,
            royalty_puzzle_hash: args.royalty_puzzle_hash,
            royalty_ten_thousandths: args.royalty_ten_thousandths,
        }))
    }

    fn construct_solution(
        &self,
        _ctx: &mut SpendContext,
        _solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        panic!("RoyaltyTransferLayer does not have a solution");
    }

    fn parse_solution(
        _allocator: &clvmr::Allocator,
        _solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        panic!("RoyaltyTransferLayer does not have a solution");
    }
}
