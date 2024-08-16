use chia_protocol::Bytes32;
use chia_puzzles::nft::{NftStateLayerArgs, NftStateLayerSolution, NFT_STATE_LAYER_PUZZLE_HASH};
use chia_sdk_types::run_puzzle;
use clvm_traits::{apply_constants, FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug)]
pub struct NftStateLayer<M, IP> {
    pub metadata: M,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub inner_puzzle: IP,
}

impl<M, IP> Layer for NftStateLayer<M, IP>
where
    IP: Layer,
    M: FromClvm<Allocator> + ToClvm<Allocator>,
{
    type Solution = NftStateLayerSolution<IP::Solution>;

    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let parent_puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(parent_puzzle) = parent_puzzle.as_curried() else {
            return Ok(None);
        };

        if parent_puzzle.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let parent_args = NftStateLayerArgs::<NodePtr, M>::from_clvm(allocator, parent_puzzle.args)
            .map_err(DriverError::FromClvm)?;

        if parent_args.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH.into() {
            return Err(DriverError::InvalidModHash);
        }

        let parent_sol = NftStateLayerSolution::<NodePtr>::from_clvm(allocator, layer_solution)
            .map_err(DriverError::FromClvm)?;

        let (metadata, metadata_updater_puzzle_hash) =
            NftStateLayer::<M, IP>::new_metadata_and_updater_from_conditions(
                allocator,
                parent_args.inner_puzzle,
                parent_sol.inner_solution,
            )?
            .unwrap_or((
                parent_args.metadata,
                parent_args.metadata_updater_puzzle_hash,
            ));

        match IP::from_parent_spend(
            allocator,
            parent_args.inner_puzzle,
            parent_sol.inner_solution,
        )? {
            None => Ok(None),
            Some(inner_puzzle) => Ok(Some(NftStateLayer::<M, IP> {
                metadata,
                metadata_updater_puzzle_hash,
                inner_puzzle,
            })),
        }
    }

    fn from_puzzle(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = NftStateLayerArgs::<NodePtr, M>::from_clvm(allocator, puzzle.args)
            .map_err(DriverError::FromClvm)?;

        if args.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH.into() {
            return Err(DriverError::InvalidModHash);
        }

        match IP::from_puzzle(allocator, args.inner_puzzle)? {
            None => Ok(None),
            Some(inner_puzzle) => Ok(Some(NftStateLayer::<M, IP> {
                metadata: args.metadata,
                metadata_updater_puzzle_hash: args.metadata_updater_puzzle_hash,
                inner_puzzle,
            })),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let metadata_ptr = self
            .metadata
            .to_clvm(ctx.allocator_mut())
            .map_err(DriverError::ToClvm)?;

        CurriedProgram {
            program: ctx.nft_state_layer().map_err(DriverError::Spend)?,
            args: NftStateLayerArgs {
                mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                metadata: metadata_ptr,
                metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
                inner_puzzle: self.inner_puzzle.construct_puzzle(ctx)?,
            },
        }
        .to_clvm(ctx.allocator_mut())
        .map_err(DriverError::ToClvm)
    }
}

impl<M, IP> ToTreeHash for NftStateLayer<M, IP>
where
    IP: ToTreeHash,
    M: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        CurriedProgram {
            program: NFT_STATE_LAYER_PUZZLE_HASH,
            args: NftStateLayerArgs {
                mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                metadata: self.metadata.tree_hash(),
                metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
                inner_puzzle: self.inner_puzzle.tree_hash(),
            },
        }
        .tree_hash()
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct NewMetadataCondition<P = NodePtr, S = NodePtr> {
    #[clvm(constant = -24)]
    pub opcode: i32,
    pub metadata_updater_reveal: P,
    pub metadata_updater_solution: S,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct NewMetadataInfo<M> {
    pub new_metadata: M,
    pub new_metadata_updater_puzhash: Bytes32,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct NewMetadataOutput<M, C> {
    pub metadata_part: NewMetadataInfo<M>,
    pub conditions: C,
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
        let output = run_puzzle(allocator, inner_layer_puzzle, inner_layer_solution)
            .map_err(DriverError::Eval)?;

        let conditions =
            Vec::<NodePtr>::from_clvm(allocator, output).map_err(DriverError::FromClvm)?;

        for condition in conditions {
            let condition =
                NewMetadataCondition::<NodePtr, NodePtr>::from_clvm(allocator, condition);

            if let Ok(condition) = condition {
                let output = run_puzzle(
                    allocator,
                    condition.metadata_updater_reveal,
                    condition.metadata_updater_solution,
                )
                .map_err(DriverError::Eval)?;

                let output = NewMetadataOutput::<M, NodePtr>::from_clvm(allocator, output)
                    .map_err(DriverError::FromClvm)?;

                return Ok(Some((
                    output.metadata_part.new_metadata,
                    output.metadata_part.new_metadata_updater_puzhash,
                )));
            }
        }

        Ok(None)
    }
}
