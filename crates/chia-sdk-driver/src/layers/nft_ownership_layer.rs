use chia_protocol::Bytes32;
use chia_puzzle_types::nft::{NftOwnershipLayerArgs, NftOwnershipLayerSolution};
use chia_puzzles::NFT_OWNERSHIP_LAYER_HASH;
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The NFT ownership [`Layer`] keeps track of the current DID that owns the NFT.
/// It also contains a transfer layer, which is used to transfer ownership of the NFT.
/// The inner puzzle layer is commonly used for determining ownership (in the key sense, not DID).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftOwnershipLayer<T, I> {
    /// The DID owner of this NFT, if it's currently assigned to one.
    pub current_owner: Option<Bytes32>,
    /// The transfer layer, which is used to transfer ownership of the NFT.
    pub transfer_layer: T,
    /// The inner puzzle layer, commonly used for determining ownership.
    pub inner_puzzle: I,
}

impl<T, I> NftOwnershipLayer<T, I> {
    pub fn new(current_owner: Option<Bytes32>, transfer_layer: T, inner_puzzle: I) -> Self {
        Self {
            current_owner,
            transfer_layer,
            inner_puzzle,
        }
    }
}

impl<T, I> Layer for NftOwnershipLayer<T, I>
where
    T: Layer,
    I: Layer,
{
    type Solution = NftOwnershipLayerSolution<I::Solution>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != NFT_OWNERSHIP_LAYER_HASH.into() {
            return Ok(None);
        }

        let args = NftOwnershipLayerArgs::<NodePtr, NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.mod_hash != NFT_OWNERSHIP_LAYER_HASH.into() {
            return Err(DriverError::InvalidModHash);
        }

        let Some(transfer_layer) =
            T::parse_puzzle(allocator, Puzzle::parse(allocator, args.transfer_program))?
        else {
            return Err(DriverError::NonStandardLayer);
        };

        let Some(inner_puzzle) =
            I::parse_puzzle(allocator, Puzzle::parse(allocator, args.inner_puzzle))?
        else {
            return Ok(None);
        };

        Ok(Some(Self {
            current_owner: args.current_owner,
            transfer_layer,
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
        let transfer_program = self.transfer_layer.construct_puzzle(ctx)?;
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        ctx.curry(NftOwnershipLayerArgs::new(
            self.current_owner,
            transfer_program,
            inner_puzzle,
        ))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let inner_solution = self
            .inner_puzzle
            .construct_solution(ctx, solution.inner_solution)?;
        ctx.alloc(&NftOwnershipLayerSolution { inner_solution })
    }
}

impl<T, I> ToTreeHash for NftOwnershipLayer<T, I>
where
    T: ToTreeHash,
    I: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        NftOwnershipLayerArgs::curry_tree_hash(
            self.current_owner,
            self.transfer_layer.tree_hash(),
            self.inner_puzzle.tree_hash(),
        )
    }
}
