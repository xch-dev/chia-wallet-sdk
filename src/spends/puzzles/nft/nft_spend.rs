use chia_protocol::{Bytes32, CoinSpend};
use chia_wallet::{
    nft::{
        NftOwnershipLayerArgs, NftOwnershipLayerSolution, NftRoyaltyTransferPuzzleArgs,
        NftStateLayerArgs, NftStateLayerSolution, NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
        NFT_STATE_LAYER_PUZZLE_HASH,
    },
    singleton::{SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH},
};
use clvm_traits::ToClvm;
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{spend_singleton, InnerSpend, NftInfo, SpendContext, SpendError};

pub fn spend_nft<M>(
    ctx: &mut SpendContext,
    nft_info: &NftInfo<M>,
    inner_spend: InnerSpend,
) -> Result<CoinSpend, SpendError>
where
    M: ToClvm<NodePtr>,
{
    let transfer_program_puzzle = ctx.nft_royalty_transfer();

    let transfer_program = CurriedProgram {
        program: transfer_program_puzzle,
        args: NftRoyaltyTransferPuzzleArgs {
            singleton_struct: SingletonStruct {
                mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
                launcher_id: nft_info.launcher_id,
                launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
            },
            royalty_puzzle_hash: nft_info.royalty_puzzle_hash,
            trade_price_percentage: nft_info.royalty_percentage,
        },
    };

    let ownership_layer_spend =
        spend_nft_ownership_layer(ctx, nft_info.current_owner, transfer_program, inner_spend)?;

    let state_layer_spend = spend_nft_state_layer(
        ctx,
        &nft_info.metadata,
        nft_info.metadata_updater_hash,
        ownership_layer_spend,
    )?;

    spend_singleton(
        ctx,
        nft_info.coin.clone(),
        nft_info.launcher_id,
        nft_info.proof.clone(),
        state_layer_spend,
    )
}

pub fn spend_nft_state_layer<M>(
    ctx: &mut SpendContext,
    metadata: M,
    metadata_updater_puzzle_hash: Bytes32,
    inner_spend: InnerSpend,
) -> Result<InnerSpend, SpendError>
where
    M: ToClvm<NodePtr>,
{
    let nft_state_layer = ctx.nft_state_layer();

    let puzzle = ctx.alloc(CurriedProgram {
        program: nft_state_layer,
        args: NftStateLayerArgs {
            mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
            metadata,
            metadata_updater_puzzle_hash,
            inner_puzzle: inner_spend.puzzle(),
        },
    })?;

    let solution = ctx.alloc(NftStateLayerSolution {
        inner_solution: inner_spend.solution(),
    })?;

    Ok(InnerSpend::new(puzzle, solution))
}

pub fn spend_nft_ownership_layer<P>(
    ctx: &mut SpendContext,
    current_owner: Option<Bytes32>,
    transfer_program: P,
    inner_spend: InnerSpend,
) -> Result<InnerSpend, SpendError>
where
    P: ToClvm<NodePtr>,
{
    let nft_ownership_layer = ctx.nft_ownership_layer();

    let puzzle = ctx.alloc(CurriedProgram {
        program: nft_ownership_layer,
        args: NftOwnershipLayerArgs {
            mod_hash: NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into(),
            current_owner,
            transfer_program,
            inner_puzzle: inner_spend.puzzle(),
        },
    })?;

    let solution = ctx.alloc(NftOwnershipLayerSolution {
        inner_solution: inner_spend.solution(),
    })?;

    Ok(InnerSpend::new(puzzle, solution))
}
