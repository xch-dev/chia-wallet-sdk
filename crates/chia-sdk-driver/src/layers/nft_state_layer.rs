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

pub struct NFTStateLayerSolution<I>
where
    I: ToClvm<NodePtr> + FromClvm<NodePtr>,
{
    pub inner_solution: I,
}

impl<M, IP, IS> PuzzleLayer<NFTStateLayerSolution<IS>> for NFTStateLayer<M, IP>
where
    IP: PuzzleLayer<IS> + ToClvm<NodePtr> + FromClvm<NodePtr>,
    M: ToClvm<NodePtr> + FromClvm<NodePtr>,
    IS: ToClvm<NodePtr> + FromClvm<NodePtr>,
{
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

        let new_metadata_cond = NFTStateLayer::<M, IP>::find_new_metadata_condition(
            allocator,
            layer_puzzle,
            layer_solution,
        )?;

        let (metadata, metadata_updater_puzzle_hash) = match new_metadata_cond {
            None => (
                parent_args.metadata,
                parent_args.metadata_updater_puzzle_hash,
            ),
            Some(new_metadata_cond) => (
                new_metadata_cond
                    .metadata_updater_solution
                    .metadata_part
                    .new_metadata,
                new_metadata_cond
                    .metadata_updater_solution
                    .metadata_part
                    .new_metadata_updater_ph,
            ),
        };

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
        solution: NFTStateLayerSolution<IS>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct DefaultMetadataSolutionMetadataList<M = NodePtr> {
    pub new_metadata: M,
    pub new_metadata_updater_ph: Bytes32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct DefaultMetadataSolution<M = NodePtr, C = NodePtr> {
    pub metadata_part: DefaultMetadataSolutionMetadataList<M>,
    pub conditions: C, // usually ()
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct NewMetadataCondition<P = NodePtr, M = NodePtr, C = NodePtr> {
    #[clvm(constant = -24)]
    pub opcode: i32,
    pub metadata_updater_reveal: P,
    pub metadata_updater_solution: DefaultMetadataSolution<M, C>,
}

impl<M, IP> NFTStateLayer<M, IP>
where
    M: FromClvm<NodePtr>,
{
    pub fn find_new_metadata_condition(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<NewMetadataCondition<NodePtr, M, NodePtr>>, ParseError> {
        let output = run_puzzle(allocator, layer_puzzle, layer_solution)
            .map_err(|err| ParseError::Eval(err))?;

        let conditions = Vec::<NodePtr>::from_clvm(allocator, output)
            .map_err(|err| ParseError::FromClvm(err))?;

        for condition in conditions {
            let condition =
                NewMetadataCondition::<NodePtr, M, NodePtr>::from_clvm(allocator, condition);

            if let Ok(condition) = condition {
                return Ok(Some(condition));
            }
        }

        Ok(None)
    }
}
