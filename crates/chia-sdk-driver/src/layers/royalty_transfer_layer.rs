use std::convert::Infallible;

use chia_protocol::Bytes32;
use chia_puzzles::{
    nft::{NftRoyaltyTransferPuzzleArgs, NFT_ROYALTY_TRANSFER_PUZZLE_HASH},
    singleton::{SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH},
};
use clvm_traits::FromClvm;
use clvm_utils::CurriedProgram;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The royalty transfer [`Layer`] is used to transfer NFTs with royalties.
/// When an NFT is transferred, a percentage of the transfer amount is paid to an address.
/// This address can for example be the creator, or a royalty split puzzle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoyaltyTransferLayer {
    /// The launcher id of the NFT this transfer program belongs to.
    pub launcher_id: Bytes32,
    /// The puzzle hash that receives royalties paid when transferring this NFT.
    pub royalty_puzzle_hash: Bytes32,
    /// The percentage of the transfer amount that is paid as royalties.
    /// This is represented in ten thousandths, so a value of 300 means 3%.
    pub royalty_ten_thousandths: u16,
}

impl RoyaltyTransferLayer {
    pub fn new(
        launcher_id: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_ten_thousandths: u16,
    ) -> Self {
        Self {
            launcher_id,
            royalty_puzzle_hash,
            royalty_ten_thousandths,
        }
    }
}

impl Layer for RoyaltyTransferLayer {
    type Solution = Infallible;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.nft_royalty_transfer()?,
            args: NftRoyaltyTransferPuzzleArgs {
                singleton_struct: SingletonStruct::new(self.launcher_id),
                royalty_puzzle_hash: self.royalty_puzzle_hash,
                royalty_ten_thousandths: self.royalty_ten_thousandths,
            },
        };
        ctx.alloc(&curried)
    }

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != NFT_ROYALTY_TRANSFER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = NftRoyaltyTransferPuzzleArgs::from_clvm(allocator, puzzle.args)?;

        if args.singleton_struct.mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH.into()
            || args.singleton_struct.launcher_puzzle_hash != SINGLETON_LAUNCHER_PUZZLE_HASH.into()
        {
            return Err(DriverError::InvalidSingletonStruct);
        }

        Ok(Some(Self {
            launcher_id: args.singleton_struct.launcher_id,
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
