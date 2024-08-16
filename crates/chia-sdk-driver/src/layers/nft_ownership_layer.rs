use chia_protocol::Bytes32;
use chia_puzzles::{
    nft::{
        NftOwnershipLayerArgs, NftOwnershipLayerSolution, NftRoyaltyTransferPuzzleArgs,
        NFT_OWNERSHIP_LAYER_PUZZLE_HASH, NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
    },
    singleton::{SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH},
};
use clvm_traits::FromClvm;
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug)]
pub struct NftOwnershipLayer<I> {
    pub launcher_id: Bytes32,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,
    pub inner_puzzle: I,
}

impl<I> Layer for NftOwnershipLayer<I>
where
    I: Layer,
{
    type Solution = NftOwnershipLayerSolution<I::Solution>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = NftOwnershipLayerArgs::<NodePtr, NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into() {
            return Err(DriverError::InvalidModHash);
        }

        let Some(transfer_puzzle) = Puzzle::parse(allocator, args.transfer_program).as_curried()
        else {
            return Err(DriverError::NonStandardLayer);
        };

        if transfer_puzzle.mod_hash != NFT_ROYALTY_TRANSFER_PUZZLE_HASH {
            return Err(DriverError::NonStandardLayer);
        }

        let transfer_args =
            NftRoyaltyTransferPuzzleArgs::from_clvm(allocator, transfer_puzzle.args)?;

        if transfer_args.singleton_struct.mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH.into()
            || transfer_args.singleton_struct.launcher_puzzle_hash
                != SINGLETON_LAUNCHER_PUZZLE_HASH.into()
        {
            return Err(DriverError::InvalidSingletonStruct);
        }

        let Some(inner_puzzle) =
            I::parse_puzzle(allocator, Puzzle::parse(allocator, args.inner_puzzle))?
        else {
            return Ok(None);
        };

        Ok(Some(Self {
            launcher_id: transfer_args.singleton_struct.launcher_id,
            current_owner: args.current_owner,
            royalty_puzzle_hash: transfer_args.royalty_puzzle_hash,
            royalty_ten_thousandths: transfer_args.royalty_ten_thousandths,
            inner_puzzle,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        let solution = NftOwnershipLayerSolution::<NodePtr>::from_clvm(allocator, solution)?;
        Ok(NftOwnershipLayerSolution {
            inner_solution: I::parse_solution(allocator, solution.inner_solution)?,
        })
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let transfer_program = CurriedProgram {
            program: ctx.nft_royalty_transfer().map_err(DriverError::Spend)?,
            args: NftRoyaltyTransferPuzzleArgs::new(
                self.launcher_id,
                self.royalty_puzzle_hash,
                self.royalty_ten_thousandths,
            ),
        };
        let curried = CurriedProgram {
            program: ctx.nft_ownership_layer().map_err(DriverError::Spend)?,
            args: NftOwnershipLayerArgs::new(
                self.current_owner,
                transfer_program,
                self.inner_puzzle.construct_puzzle(ctx)?,
            ),
        };
        Ok(ctx.alloc(&curried)?)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let inner_solution = self
            .inner_puzzle
            .construct_solution(ctx, solution.inner_solution)?;
        Ok(ctx.alloc(&NftOwnershipLayerSolution { inner_solution })?)
    }
}

impl<IP> ToTreeHash for NftOwnershipLayer<IP>
where
    IP: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        NftOwnershipLayerArgs::curry_tree_hash(
            self.current_owner,
            NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
                self.launcher_id,
                self.royalty_puzzle_hash,
                self.royalty_ten_thousandths,
            ),
            self.inner_puzzle.tree_hash(),
        )
    }
}
