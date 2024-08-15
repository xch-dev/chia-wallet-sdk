use chia_protocol::Bytes32;
use chia_puzzles::{
    nft::{
        NftOwnershipLayerArgs, NftOwnershipLayerSolution, NftRoyaltyTransferPuzzleArgs,
        NFT_OWNERSHIP_LAYER_PUZZLE_HASH, NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
    },
    singleton::SingletonStruct,
};
use chia_sdk_types::conditions::{run_puzzle, NewNftOwner};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug)]
pub struct NftOwnershipLayer<IP> {
    pub current_owner: Option<Bytes32>,
    pub launcher_id: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,
    pub inner_puzzle: IP,
}

impl<IP> Layer for NftOwnershipLayer<IP>
where
    IP: Layer,
{
    type Solution = NftOwnershipLayerSolution<IP::Solution>;

    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let parent_puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(parent_puzzle) = parent_puzzle.as_curried() else {
            return Ok(None);
        };

        if parent_puzzle.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let parent_args =
            NftOwnershipLayerArgs::<NodePtr, NodePtr>::from_clvm(allocator, parent_puzzle.args)
                .map_err(DriverError::FromClvm)?;

        if parent_args.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into() {
            return Err(DriverError::InvalidModHash);
        }

        let parent_sol = NftOwnershipLayerSolution::<NodePtr>::from_clvm(allocator, layer_solution)
            .map_err(DriverError::FromClvm)?;

        let new_owner_maybe = NftOwnershipLayer::<IP>::new_owner_from_conditions(
            allocator,
            parent_args.inner_puzzle,
            parent_sol.inner_solution,
        )?;

        let Some(parent_transfer_puzzle) =
            Puzzle::parse(allocator, parent_args.transfer_program).as_curried()
        else {
            return Err(DriverError::NonStandardLayer);
        };

        if parent_transfer_puzzle.mod_hash != NFT_ROYALTY_TRANSFER_PUZZLE_HASH {
            return Err(DriverError::NonStandardLayer);
        }

        let parent_transfer_args =
            NftRoyaltyTransferPuzzleArgs::from_clvm(allocator, parent_transfer_puzzle.args)?;

        match IP::from_parent_spend(
            allocator,
            parent_args.inner_puzzle,
            parent_sol.inner_solution,
        )? {
            None => Ok(None),
            Some(inner_puzzle) => Ok(Some(NftOwnershipLayer::<IP> {
                launcher_id: parent_transfer_args.singleton_struct.launcher_id,
                current_owner: new_owner_maybe.unwrap_or(parent_args.current_owner),
                royalty_puzzle_hash: parent_transfer_args.royalty_puzzle_hash,
                royalty_ten_thousandths: parent_transfer_args.royalty_ten_thousandths,
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

        if puzzle.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = NftOwnershipLayerArgs::<NodePtr, NodePtr>::from_clvm(allocator, puzzle.args)
            .map_err(DriverError::FromClvm)?;

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

        match IP::from_puzzle(allocator, args.inner_puzzle)? {
            None => Ok(None),
            Some(inner_puzzle) => Ok(Some(NftOwnershipLayer::<IP> {
                current_owner: args.current_owner,
                launcher_id: transfer_args.singleton_struct.launcher_id,
                royalty_puzzle_hash: transfer_args.royalty_puzzle_hash,
                royalty_ten_thousandths: transfer_args.royalty_ten_thousandths,
                inner_puzzle,
            })),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let transfer_program = CurriedProgram {
            program: ctx.nft_royalty_transfer().map_err(DriverError::Spend)?,
            args: NftRoyaltyTransferPuzzleArgs {
                singleton_struct: SingletonStruct::new(self.launcher_id),
                royalty_puzzle_hash: self.royalty_puzzle_hash,
                royalty_ten_thousandths: self.royalty_ten_thousandths,
            },
        }
        .to_clvm(ctx.allocator_mut())
        .map_err(DriverError::ToClvm)?;

        CurriedProgram {
            program: ctx.nft_ownership_layer().map_err(DriverError::Spend)?,
            args: NftOwnershipLayerArgs {
                mod_hash: NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into(),
                current_owner: self.current_owner,
                transfer_program,
                inner_puzzle: self.inner_puzzle.construct_puzzle(ctx)?,
            },
        }
        .to_clvm(ctx.allocator_mut())
        .map_err(DriverError::ToClvm)
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

impl<IP> NftOwnershipLayer<IP> {
    pub fn new_owner_from_conditions(
        allocator: &mut Allocator,
        inner_layer_puzzle: NodePtr,
        inner_layer_solution: NodePtr,
    ) -> Result<Option<Option<Bytes32>>, DriverError> {
        let output = run_puzzle(allocator, inner_layer_puzzle, inner_layer_solution)
            .map_err(DriverError::Eval)?;

        let conditions =
            Vec::<NodePtr>::from_clvm(allocator, output).map_err(DriverError::FromClvm)?;

        for condition in conditions {
            let condition = NewNftOwner::from_clvm(allocator, condition);

            if let Ok(condition) = condition {
                return Ok(Some(condition.did_id));
            }
        }

        Ok(None)
    }
}
