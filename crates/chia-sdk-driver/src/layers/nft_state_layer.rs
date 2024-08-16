use chia_protocol::Bytes32;
use chia_puzzles::nft::{NftStateLayerArgs, NftStateLayerSolution, NFT_STATE_LAYER_PUZZLE_HASH};
use chia_sdk_types::{run_puzzle, NewMetadataCondition, NewMetadataOutput};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug)]
pub struct NftStateLayer<M, I> {
    pub metadata: M,
    pub metadata_updater_puzzle_hash: Bytes32,
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

        if puzzle.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = NftStateLayerArgs::<NodePtr, M>::from_clvm(allocator, puzzle.args)?;

        if args.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH.into() {
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
        let curried = CurriedProgram {
            program: ctx.nft_state_layer()?,
            args: NftStateLayerArgs {
                mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                metadata: &self.metadata,
                metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
                inner_puzzle: self.inner_puzzle.construct_puzzle(ctx)?,
            },
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
        Ok(ctx.alloc(&NftStateLayerSolution { inner_solution })?)
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
            program: NFT_STATE_LAYER_PUZZLE_HASH,
            args: NftStateLayerArgs {
                mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                metadata: metadata_hash,
                metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
                inner_puzzle: inner_puzzle_hash,
            },
        }
        .tree_hash()
    }
}

impl<M, IP> NftStateLayer<M, IP>
where
    M: FromClvm<Allocator>,
{
    pub fn new_metadata_and_updater_from_conditions(
        allocator: &mut Allocator,
        inner_layer_puzzle: NodePtr,
        inner_layer_solution: NodePtr,
    ) -> Result<Option<(M, Bytes32)>, DriverError> {
        let output = run_puzzle(allocator, inner_layer_puzzle, inner_layer_solution)?;

        let conditions = Vec::<NodePtr>::from_clvm(allocator, output)?;

        for condition in conditions {
            let condition =
                NewMetadataCondition::<NodePtr, NodePtr>::from_clvm(allocator, condition);

            if let Ok(condition) = condition {
                let output = run_puzzle(
                    allocator,
                    condition.metadata_updater_reveal,
                    condition.metadata_updater_solution,
                )?;

                let output = NewMetadataOutput::<M, NodePtr>::from_clvm(allocator, output)?;

                return Ok(Some((
                    output.metadata_part.new_metadata,
                    output.metadata_part.new_metadata_updater_puzhash,
                )));
            }
        }

        Ok(None)
    }
}
