use chia_protocol::{Bytes32, Coin};
use chia_puzzles::{
    nft::{
        NftOwnershipLayerArgs, NftOwnershipLayerSolution, NftRoyaltyTransferPuzzleArgs,
        NftStateLayerArgs, NftStateLayerSolution, NFT_METADATA_UPDATER_PUZZLE_HASH,
        NFT_OWNERSHIP_LAYER_PUZZLE_HASH, NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
        NFT_STATE_LAYER_PUZZLE_HASH,
    },
    singleton::{SingletonArgs, SingletonStruct},
    Proof,
};
use chia_sdk_types::{
    conditions::{puzzle_conditions, Condition, CreateCoin, NewNftOwner},
    puzzles::NftInfo,
};
use clvm_traits::FromClvm;
use clvm_utils::tree_hash;
use clvmr::{Allocator, NodePtr};

use crate::{ParseError, Puzzle, SingletonPuzzle};

#[derive(Debug, Clone, Copy)]
pub struct NftPuzzle {
    pub p2_puzzle: Puzzle,
    pub metadata: NodePtr,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_percentage: u16,
}

impl NftPuzzle {
    pub fn parse(
        allocator: &Allocator,
        launcher_id: Bytes32,
        puzzle: &Puzzle,
    ) -> Result<Option<Self>, ParseError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let state_args = NftStateLayerArgs::<NodePtr, NodePtr>::from_clvm(allocator, puzzle.args)?;

        if state_args.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH.into() {
            return Err(ParseError::InvalidModHash);
        }

        if state_args.metadata_updater_puzzle_hash != NFT_METADATA_UPDATER_PUZZLE_HASH.into() {
            return Err(ParseError::NonStandardLayer);
        }

        let Some(inner_puzzle) = Puzzle::parse(allocator, state_args.inner_puzzle).as_curried()
        else {
            return Err(ParseError::NonStandardLayer);
        };

        let ownership_args =
            NftOwnershipLayerArgs::<NodePtr, NodePtr>::from_clvm(allocator, inner_puzzle.args)?;

        if ownership_args.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into() {
            return Err(ParseError::NonStandardLayer);
        }

        let Some(transfer_puzzle) =
            Puzzle::parse(allocator, ownership_args.transfer_program).as_curried()
        else {
            return Err(ParseError::NonStandardLayer);
        };

        if transfer_puzzle.mod_hash != NFT_ROYALTY_TRANSFER_PUZZLE_HASH {
            return Err(ParseError::NonStandardLayer);
        }

        let transfer_args =
            NftRoyaltyTransferPuzzleArgs::from_clvm(allocator, transfer_puzzle.args)?;

        if transfer_args.singleton_struct != SingletonStruct::new(launcher_id) {
            return Err(ParseError::InvalidSingletonStruct);
        }

        Ok(Some(Self {
            p2_puzzle: Puzzle::parse(allocator, ownership_args.inner_puzzle),
            metadata: state_args.metadata,
            current_owner: ownership_args.current_owner,
            royalty_puzzle_hash: transfer_args.royalty_puzzle_hash,
            royalty_percentage: transfer_args.trade_price_percentage,
        }))
    }

    pub fn output(
        &self,
        allocator: &mut Allocator,
        solution: NodePtr,
    ) -> Result<(Option<CreateCoin>, Option<NewNftOwner>), ParseError> {
        let state_solution = NftStateLayerSolution::from_clvm(allocator, solution)?;
        let ownership_solution =
            NftOwnershipLayerSolution::from_clvm(allocator, state_solution.inner_solution)?;

        let conditions = puzzle_conditions(
            allocator,
            self.p2_puzzle.ptr(),
            ownership_solution.inner_solution,
        )?;

        let mut create_coin = None;
        let mut new_nft_owner = None;

        for condition in conditions {
            match condition {
                Condition::CreateCoin(condition) => {
                    create_coin = Some(condition);
                }
                Condition::Other(condition) => {
                    if let Ok(condition) = NewNftOwner::from_clvm(allocator, condition) {
                        new_nft_owner = Some(condition);
                    }
                }
                _ => {}
            }
        }

        Ok((create_coin, new_nft_owner))
    }

    pub fn child_coin_info(
        &self,
        allocator: &mut Allocator,
        singleton: &SingletonPuzzle,
        parent_coin: Coin,
        child_coin: Coin,
        solution: NodePtr,
    ) -> Result<NftInfo<NodePtr>, ParseError> {
        let (create_coin, new_owner) = self.output(allocator, solution)?;
        let create_coin = create_coin.ok_or(ParseError::MissingChild)?;

        let current_owner = if let Some(condition) = new_owner {
            condition.did_id
        } else {
            self.current_owner
        };

        let transfer = NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
            singleton.launcher_id,
            self.royalty_puzzle_hash,
            self.royalty_percentage,
        );

        let ownership = NftOwnershipLayerArgs::curry_tree_hash(
            current_owner,
            transfer,
            create_coin.puzzle_hash.into(),
        );

        let state =
            NftStateLayerArgs::curry_tree_hash(tree_hash(allocator, self.metadata), ownership);

        let singleton_puzzle_hash = SingletonArgs::curry_tree_hash(singleton.launcher_id, state);

        if singleton_puzzle_hash != child_coin.puzzle_hash.into() {
            return Err(ParseError::MismatchedOutput);
        }

        Ok(NftInfo {
            launcher_id: singleton.launcher_id,
            coin: child_coin,
            p2_puzzle_hash: create_coin.puzzle_hash,
            inner_puzzle_hash: state.into(),
            royalty_percentage: self.royalty_percentage,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            current_owner,
            metadata: self.metadata,
            metadata_hash: tree_hash(allocator, self.metadata),
            proof: Proof::Lineage(singleton.lineage_proof(parent_coin)),
        })
    }
}
