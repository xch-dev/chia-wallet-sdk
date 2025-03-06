use chia_protocol::Bytes32;
use chia_puzzle_types::nft::{NftStateLayerArgs, NftStateLayerSolution};
use chia_puzzles::NFT_STATE_LAYER_HASH;
use chia_sdk_types::{
    conditions::{NewMetadataOutput, UpdateNftMetadata},
    run_puzzle,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The NFT state [`Layer`] keeps track of the current metadata of the NFT and how to change it.
/// It's typically an inner layer of the [`SingletonLayer`](crate::SingletonLayer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftStateLayer<M, I> {
    /// The NFT metadata. The standard metadata type is [`NftMetadata`](chia_puzzle_types::nft::NftMetadata).
    pub metadata: M,
    /// The tree hash of the metadata updater puzzle.
    pub metadata_updater_puzzle_hash: Bytes32,
    /// The inner puzzle layer. Typically, this is the [`NftOwnershipLayer`](crate::NftOwnershipLayer).
    /// However, for the NFT0 standard this can be the p2 layer itself.
    pub inner_puzzle: I,
}

impl<M, I> NftStateLayer<M, I> {
    pub fn new(metadata: M, metadata_updater_puzzle_hash: Bytes32, inner_puzzle: I) -> Self {
        Self {
            metadata,
            metadata_updater_puzzle_hash,
            inner_puzzle,
        }
    }

    pub fn with_metadata<N>(self, metadata: N) -> NftStateLayer<N, I> {
        NftStateLayer {
            metadata,
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            inner_puzzle: self.inner_puzzle,
        }
    }
}

impl<M, I> Layer for NftStateLayer<M, I>
where
    M: ToClvm<Allocator> + FromClvm<Allocator>,
    I: Layer,
{
    type Solution = NftStateLayerSolution<I::Solution>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != NFT_STATE_LAYER_HASH.into() {
            return Ok(None);
        }

        let args = NftStateLayerArgs::<NodePtr, M>::from_clvm(allocator, puzzle.args)?;

        if args.mod_hash != NFT_STATE_LAYER_HASH.into() {
            return Err(DriverError::InvalidModHash);
        }

        let Some(inner_puzzle) =
            I::parse_puzzle(allocator, Puzzle::parse(allocator, args.inner_puzzle))?
        else {
            return Ok(None);
        };

        Ok(Some(Self {
            metadata: args.metadata,
            metadata_updater_puzzle_hash: args.metadata_updater_puzzle_hash,
            inner_puzzle,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        let solution = NftStateLayerSolution::<NodePtr>::from_clvm(allocator, solution)?;
        Ok(NftStateLayerSolution {
            inner_solution: I::parse_solution(allocator, solution.inner_solution)?,
        })
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        ctx.curry(NftStateLayerArgs {
            mod_hash: NFT_STATE_LAYER_HASH.into(),
            metadata: &self.metadata,
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            inner_puzzle,
        })
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let inner_solution = self
            .inner_puzzle
            .construct_solution(ctx, solution.inner_solution)?;
        ctx.alloc(&NftStateLayerSolution { inner_solution })
    }
}

impl<M, I> ToTreeHash for NftStateLayer<M, I>
where
    M: ToTreeHash,
    I: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        let metadata_hash = self.metadata.tree_hash();
        let inner_puzzle_hash = self.inner_puzzle.tree_hash();
        CurriedProgram {
            program: TreeHash::new(NFT_STATE_LAYER_HASH),
            args: NftStateLayerArgs {
                mod_hash: NFT_STATE_LAYER_HASH.into(),
                metadata: metadata_hash,
                metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
                inner_puzzle: inner_puzzle_hash,
            },
        }
        .tree_hash()
    }
}

impl<M, I> NftStateLayer<M, I> {
    pub fn get_next_metadata(
        allocator: &mut Allocator,
        current_metadata: &M,
        curent_metadata_updater_puzzle_hash: Bytes32,
        condition: UpdateNftMetadata<NodePtr, NodePtr>,
    ) -> Result<M, DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator>,
    {
        let real_metadata_updater_solution: Vec<NodePtr> = vec![
            current_metadata.to_clvm(allocator)?,
            curent_metadata_updater_puzzle_hash.to_clvm(allocator)?,
            condition.updater_solution,
        ];
        let real_metadata_updater_solution = real_metadata_updater_solution.to_clvm(allocator)?;

        let output = run_puzzle(
            allocator,
            condition.updater_puzzle_reveal,
            real_metadata_updater_solution,
        )?;

        let parsed = NewMetadataOutput::<M, NodePtr>::from_clvm(allocator, output)?;

        Ok(parsed.metadata_info.new_metadata)
    }
}
