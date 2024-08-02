use chia_protocol::Bytes32;
use chia_puzzles::{
    nft::{
        NftOwnershipLayerArgs, NftRoyaltyTransferPuzzleArgs, NftStateLayerArgs,
        NFT_OWNERSHIP_LAYER_PUZZLE_HASH, NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
        NFT_STATE_LAYER_PUZZLE_HASH,
    },
    singleton::SingletonStruct,
};
use chia_sdk_types::conditions::run_puzzle;
use clvm_traits::{apply_constants, FromClvm, ToClvm, ToNodePtr};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{ParseError, Puzzle, PuzzleLayer, SpendContext};

#[derive(Debug)]

pub struct NFTOwnershipLayer<IP> {
    pub current_owner: Option<Bytes32>,
    pub launcher_id: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_percentage: u16,
    pub inner_puzzle: IP,
}

#[derive(Debug, ToClvm, FromClvm)]
#[clvm(list)]

pub struct NFTOwnershipLayerSolution<I>
where
    I: ToClvm<NodePtr> + FromClvm<NodePtr>,
{
    pub inner_solution: I,
}

impl<IP, IS> PuzzleLayer<NFTOwnershipLayerSolution<IS>> for NFTOwnershipLayer<IP>
where
    IP: PuzzleLayer<IS> + ToClvm<NodePtr> + FromClvm<NodePtr>,
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

        if parent_puzzle.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let parent_args =
            NftOwnershipLayerArgs::<NodePtr, NodePtr>::from_clvm(allocator, parent_puzzle.args)
                .map_err(|err| ParseError::FromClvm(err))?;

        if parent_args.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into() {
            return Err(ParseError::InvalidModHash);
        }

        // let new_metadata_cond = NFTStateLayer::<M, IP>::find_new_metadata_condition(
        //     allocator,
        //     layer_puzzle,
        //     layer_solution,
        // )?;

        // let (metadata, metadata_updater_puzzle_hash) = match new_metadata_cond {
        //     None => (
        //         parent_args.metadata,
        //         parent_args.metadata_updater_puzzle_hash,
        //     ),
        //     Some(new_metadata_cond) => (
        //         new_metadata_cond
        //             .metadata_updater_solution
        //             .metadata_part
        //             .new_metadata,
        //         new_metadata_cond
        //             .metadata_updater_solution
        //             .metadata_part
        //             .new_metadata_updater_ph,
        //     ),
        // };

        // let parent_sol = NftStateLayerSolution::<NodePtr>::from_clvm(allocator, layer_solution)
        //     .map_err(|err| ParseError::FromClvm(err))?;

        // match IP::from_parent_spend(
        //     allocator,
        //     parent_args.inner_puzzle,
        //     parent_sol.inner_solution,
        // )? {
        //     None => return Ok(None),
        //     Some(inner_puzzle) => Ok(Some(NFTStateLayer::<M, IP> {
        //         metadata,
        //         metadata_updater_puzzle_hash,
        //         inner_puzzle,
        //     })),
        // }
        todo!("parse conds and figure out transfer")
    }

    fn from_puzzle(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        let puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = NftOwnershipLayerArgs::<NodePtr, NodePtr>::from_clvm(allocator, puzzle.args)
            .map_err(|err| ParseError::FromClvm(err))?;

        if args.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into() {
            return Err(ParseError::InvalidModHash);
        }

        let Some(transfer_puzzle) = Puzzle::parse(allocator, args.transfer_program).as_curried()
        else {
            return Err(ParseError::NonStandardLayer);
        };

        if transfer_puzzle.mod_hash != NFT_ROYALTY_TRANSFER_PUZZLE_HASH {
            return Err(ParseError::NonStandardLayer);
        }

        let transfer_args =
            NftRoyaltyTransferPuzzleArgs::from_clvm(allocator, transfer_puzzle.args)?;

        match IP::from_puzzle(allocator, args.inner_puzzle)? {
            None => return Ok(None),
            Some(inner_puzzle) => Ok(Some(NFTOwnershipLayer::<IP> {
                current_owner: args.current_owner,
                launcher_id: transfer_args.singleton_struct.launcher_id,
                royalty_puzzle_hash: transfer_args.royalty_puzzle_hash,
                royalty_percentage: transfer_args.trade_price_percentage,
                inner_puzzle,
            })),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, ParseError> {
        let transfer_program = CurriedProgram {
            program: ctx
                .nft_royalty_transfer()
                .map_err(|err| ParseError::Spend(err))?,
            args: NftRoyaltyTransferPuzzleArgs {
                singleton_struct: SingletonStruct::new(self.launcher_id),
                royalty_puzzle_hash: self.royalty_puzzle_hash,
                trade_price_percentage: self.royalty_percentage,
            },
        }
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| ParseError::ToClvm(err))?;

        CurriedProgram {
            program: ctx
                .nft_ownership_layer()
                .map_err(|err| ParseError::Spend(err))?,
            args: NftOwnershipLayerArgs {
                mod_hash: NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into(),
                current_owner: self.current_owner,
                transfer_program: transfer_program,
                inner_puzzle: self.inner_puzzle.construct_puzzle(ctx)?,
            },
        }
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| ParseError::ToClvm(err))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: NFTOwnershipLayerSolution<IS>,
    ) -> Result<NodePtr, ParseError> {
        NFTOwnershipLayerSolution {
            inner_solution: self
                .inner_puzzle
                .construct_solution(ctx, solution.inner_solution)?,
        }
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| ParseError::ToClvm(err))
    }
}

impl<IP> ToTreeHash for NFTOwnershipLayer<IP>
where
    IP: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        NftOwnershipLayerArgs::curry_tree_hash(
            self.current_owner,
            NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
                self.launcher_id,
                self.royalty_puzzle_hash,
                self.royalty_percentage,
            ),
            self.inner_puzzle.tree_hash(),
        )
    }
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
