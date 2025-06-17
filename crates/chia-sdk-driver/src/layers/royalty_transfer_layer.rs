use std::convert::Infallible;

use chia_protocol::Bytes32;
use chia_puzzle_types::{nft::NftRoyaltyTransferPuzzleArgs, singleton::SingletonStruct};
use chia_puzzles::{
    NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH, SINGLETON_LAUNCHER_HASH,
    SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
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
    pub royalty_basis_points: u16,
}

impl RoyaltyTransferLayer {
    pub fn new(
        launcher_id: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
    ) -> Self {
        Self {
            launcher_id,
            royalty_puzzle_hash,
            royalty_basis_points,
        }
    }
}

impl Layer for RoyaltyTransferLayer {
    type Solution = Infallible;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(NftRoyaltyTransferPuzzleArgs {
            singleton_struct: SingletonStruct::new(self.launcher_id),
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_ten_thousandths: self.royalty_basis_points,
        })
    }

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash
            != NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH.into()
        {
            return Ok(None);
        }

        let args = NftRoyaltyTransferPuzzleArgs::from_clvm(allocator, puzzle.args)?;

        if args.singleton_struct.mod_hash != SINGLETON_TOP_LAYER_V1_1_HASH.into()
            || args.singleton_struct.launcher_puzzle_hash != SINGLETON_LAUNCHER_HASH.into()
        {
            return Err(DriverError::InvalidSingletonStruct);
        }

        Ok(Some(Self {
            launcher_id: args.singleton_struct.launcher_id,
            royalty_puzzle_hash: args.royalty_puzzle_hash,
            royalty_basis_points: args.royalty_ten_thousandths,
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

impl ToTreeHash for RoyaltyTransferLayer {
    fn tree_hash(&self) -> TreeHash {
        NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
            self.launcher_id,
            self.royalty_puzzle_hash,
            self.royalty_basis_points,
        )
    }
}
