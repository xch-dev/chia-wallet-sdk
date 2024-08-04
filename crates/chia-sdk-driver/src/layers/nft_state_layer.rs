use chia_protocol::Bytes32;
use chia_puzzles::nft::{NftStateLayerArgs, NftStateLayerSolution, NFT_STATE_LAYER_PUZZLE_HASH};
use chia_sdk_types::conditions::run_puzzle;
use clvm_traits::{apply_constants, FromClvm, ToClvm, ToNodePtr};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{ParseError, Puzzle, PuzzleLayer, SpendContext};

#[derive(Debug)]

pub struct NFTStateLayer<M, IP> {
    pub metadata: M,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub inner_puzzle: IP,
}

#[derive(Debug, ToClvm, FromClvm)]
#[clvm(list)]

pub struct NFTStateLayerSolution<I> {
    pub inner_solution: I,
}

impl<M, IP> PuzzleLayer for NFTStateLayer<M, IP>
where
    IP: PuzzleLayer,
    M: FromClvm<NodePtr> + ToClvm<NodePtr>,
{
    type Solution = NFTStateLayerSolution<IP::Solution>;

    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        let parent_puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(parent_puzzle) = parent_puzzle.as_curried() else {
            return Ok(None);
        };

        if parent_puzzle.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let parent_args = NftStateLayerArgs::<NodePtr, M>::from_clvm(allocator, parent_puzzle.args)
            .map_err(|err| ParseError::FromClvm(err))?;

        if parent_args.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH.into() {
            return Err(ParseError::InvalidModHash);
        }

        let (metadata, metadata_updater_puzzle_hash) =
            NFTStateLayer::<M, IP>::new_metadata_and_updater_from_conditions(
                allocator,
                layer_puzzle,
                layer_solution,
            )?
            .unwrap_or((
                parent_args.metadata,
                parent_args.metadata_updater_puzzle_hash,
            ));

        let parent_sol = NftStateLayerSolution::<NodePtr>::from_clvm(allocator, layer_solution)
            .map_err(|err| ParseError::FromClvm(err))?;

        match IP::from_parent_spend(
            allocator,
            parent_args.inner_puzzle,
            parent_sol.inner_solution,
        )? {
            None => return Ok(None),
            Some(inner_puzzle) => Ok(Some(NFTStateLayer::<M, IP> {
                metadata,
                metadata_updater_puzzle_hash,
                inner_puzzle,
            })),
        }
    }

    fn from_puzzle(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        let puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = NftStateLayerArgs::<NodePtr, M>::from_clvm(allocator, puzzle.args)
            .map_err(|err| ParseError::FromClvm(err))?;

        if args.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH.into() {
            return Err(ParseError::InvalidModHash);
        }

        match IP::from_puzzle(allocator, args.inner_puzzle)? {
            None => return Ok(None),
            Some(inner_puzzle) => Ok(Some(NFTStateLayer::<M, IP> {
                metadata: args.metadata,
                metadata_updater_puzzle_hash: args.metadata_updater_puzzle_hash,
                inner_puzzle,
            })),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, ParseError> {
        let metadata_ptr = self
            .metadata
            .to_node_ptr(ctx.allocator_mut())
            .map_err(|err| ParseError::ToClvm(err))?;

        CurriedProgram {
            program: ctx
                .nft_state_layer()
                .map_err(|err| ParseError::Spend(err))?,
            args: NftStateLayerArgs {
                mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                metadata: metadata_ptr,
                metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
                inner_puzzle: self.inner_puzzle.construct_puzzle(ctx)?,
            },
        }
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| ParseError::ToClvm(err))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, ParseError> {
        NFTStateLayerSolution {
            inner_solution: self
                .inner_puzzle
                .construct_solution(ctx, solution.inner_solution)?,
        }
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| ParseError::ToClvm(err))
    }
}

impl<M, IP> ToTreeHash for NFTStateLayer<M, IP>
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

impl<M, IP> NFTStateLayer<M, IP>
where
    M: FromClvm<NodePtr>,
{
    pub fn new_metadata_and_updater_from_conditions(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<(M, Bytes32)>, ParseError> {
        let output = run_puzzle(allocator, layer_puzzle, layer_solution)
            .map_err(|err| ParseError::Eval(err))?;

        let conditions = Vec::<NodePtr>::from_clvm(allocator, output)
            .map_err(|err| ParseError::FromClvm(err))?;

        for condition in conditions {
            let condition =
                NewMetadataCondition::<NodePtr, NodePtr>::from_clvm(allocator, condition);

            if let Ok(condition) = condition {
                let output = run_puzzle(
                    allocator,
                    condition.metadata_updater_reveal,
                    condition.metadata_updater_solution,
                )
                .map_err(|err| ParseError::Eval(err))?;

                let output = NewMetadataOutput::<M, NodePtr>::from_clvm(allocator, output)
                    .map_err(|err| ParseError::FromClvm(err))?;

                return Ok(Some((
                    output.metadata_part.new_metadata,
                    output.metadata_part.new_metadata_updater_puzhash,
                )));
            }
        }

        Ok(None)
    }
}
