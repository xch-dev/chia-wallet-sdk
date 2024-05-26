use chia_puzzles::{
    nft::{
        NftOwnershipLayerArgs, NftOwnershipLayerSolution, NftRoyaltyTransferPuzzleArgs,
        NftStateLayerArgs, NftStateLayerSolution, NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
        NFT_ROYALTY_TRANSFER_PUZZLE_HASH, NFT_STATE_LAYER_PUZZLE_HASH,
    },
    singleton::SingletonArgs,
    LineageProof, Proof,
};
use chia_sdk_types::{
    conditions::{CreateCoin, NewNftOwner},
    puzzles::NftInfo,
};
use clvm_traits::FromClvm;
use clvm_utils::{tree_hash, CurriedProgram};
use clvmr::{reduction::Reduction, run_program, Allocator, ChiaDialect, NodePtr};

use crate::{ParseContext, ParseError, ParseSingleton};

pub fn parse_nft(
    allocator: &mut Allocator,
    ctx: &ParseContext,
    singleton: &ParseSingleton,
    max_cost: u64,
) -> Result<Option<NftInfo<NodePtr>>, ParseError> {
    if singleton.inner_mod_hash().to_bytes() != NFT_STATE_LAYER_PUZZLE_HASH.to_bytes() {
        return Ok(None);
    }

    let state =
        NftStateLayerArgs::<NodePtr, NodePtr>::from_clvm(allocator, singleton.inner_args())?;
    let state_solution =
        NftStateLayerSolution::<NodePtr>::from_clvm(allocator, singleton.inner_solution())?;

    let curried_ownership =
        CurriedProgram::<NodePtr, NodePtr>::from_clvm(allocator, state.inner_puzzle)?;

    if tree_hash(allocator, curried_ownership.program).to_bytes()
        != NFT_OWNERSHIP_LAYER_PUZZLE_HASH.to_bytes()
    {
        return Ok(None);
    }

    let ownership =
        NftOwnershipLayerArgs::<NodePtr, NodePtr>::from_clvm(allocator, curried_ownership.args)?;
    let ownership_solution =
        NftOwnershipLayerSolution::<NodePtr>::from_clvm(allocator, state_solution.inner_solution)?;

    let curried_transfer =
        CurriedProgram::<NodePtr, NodePtr>::from_clvm(allocator, ownership.transfer_program)?;

    if tree_hash(allocator, curried_transfer.program).to_bytes()
        != NFT_ROYALTY_TRANSFER_PUZZLE_HASH.to_bytes()
    {
        return Ok(None);
    }

    let transfer = NftRoyaltyTransferPuzzleArgs::from_clvm(allocator, curried_transfer.args)?;

    if transfer.singleton_struct != singleton.args().singleton_struct {
        return Err(ParseError::SingletonStructMismatch);
    }

    let Reduction(_cost, output) = run_program(
        allocator,
        &ChiaDialect::new(0),
        ownership.inner_puzzle,
        ownership_solution.inner_solution,
        max_cost,
    )?;

    let conditions = Vec::<NodePtr>::from_clvm(allocator, output)?;

    let mut p2_puzzle_hash = None;
    let mut current_owner = ownership.current_owner;

    for condition in conditions {
        if let Ok(new_owner_condition) = NewNftOwner::from_clvm(allocator, condition) {
            current_owner = new_owner_condition.new_owner;
        }

        let Ok(create_coin) = CreateCoin::from_clvm(allocator, condition) else {
            continue;
        };

        if create_coin.amount % 2 == 0 {
            continue;
        }

        p2_puzzle_hash = Some(create_coin.puzzle_hash);
        break;
    }

    let Some(p2_puzzle_hash) = p2_puzzle_hash else {
        return Err(ParseError::MissingCreateCoin);
    };

    let ownership_puzzle_hash = NftOwnershipLayerArgs::curry_tree_hash(
        current_owner,
        tree_hash(allocator, ownership.transfer_program),
        p2_puzzle_hash.into(),
    );

    let state_puzzle_hash = NftStateLayerArgs::curry_tree_hash(
        tree_hash(allocator, state.metadata),
        ownership_puzzle_hash,
    );

    let singleton_puzzle_hash = SingletonArgs::curry_tree_hash(
        singleton.args().singleton_struct.launcher_id,
        state_puzzle_hash,
    );

    if singleton_puzzle_hash.to_bytes() != ctx.coin().puzzle_hash.to_bytes() {
        return Err(ParseError::UnknownOutput);
    }

    Ok(Some(NftInfo {
        launcher_id: singleton.args().singleton_struct.launcher_id,
        coin: ctx.coin(),
        p2_puzzle_hash,
        nft_inner_puzzle_hash: state_puzzle_hash.into(),
        metadata_updater_hash: state.metadata_updater_puzzle_hash,
        royalty_percentage: transfer.trade_price_percentage,
        royalty_puzzle_hash: transfer.royalty_puzzle_hash,
        current_owner,
        metadata: state.metadata,
        proof: Proof::Lineage(LineageProof {
            parent_parent_coin_id: ctx.parent_coin().parent_coin_info,
            parent_inner_puzzle_hash: tree_hash(allocator, singleton.args().inner_puzzle).into(),
            parent_amount: ctx.parent_coin().amount,
        }),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use chia_bls::PublicKey;
    use chia_protocol::{Bytes32, Coin};
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_driver::{
        puzzles::{CreateDid, Launcher, MintNft, StandardMint, StandardSpend},
        SpendContext,
    };
    use clvm_traits::ToNodePtr;

    use crate::{parse_puzzle, parse_singleton};

    #[test]
    fn test_parse_nft() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let pk = PublicKey::default();
        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let parent = Coin::new(Bytes32::default(), puzzle_hash, 2);

        let (create_did, did_info) = Launcher::new(parent.coin_id(), 1)
            .create(ctx)?
            .create_standard_did(ctx, pk)?;

        let (mint_nft, nft_info) = Launcher::new(did_info.coin.coin_id(), 1)
            .create(ctx)?
            .mint_standard_nft(
                ctx,
                StandardMint {
                    metadata: (),
                    royalty_percentage: 300,
                    royalty_puzzle_hash: Bytes32::new([1; 32]),
                    owner_puzzle_hash: puzzle_hash,
                    synthetic_key: pk,
                    did_id: did_info.launcher_id,
                    did_inner_puzzle_hash: did_info.did_inner_puzzle_hash,
                },
            )?;

        StandardSpend::new()
            .chain(create_did)
            .chain(mint_nft)
            .finish(ctx, parent, pk)?;

        let coin_spends = ctx.take_spends();

        let coin_spend = coin_spends
            .into_iter()
            .find(|cs| cs.coin.coin_id() == nft_info.coin.parent_coin_info)
            .unwrap();

        let puzzle = coin_spend.puzzle_reveal.to_node_ptr(&mut allocator)?;
        let solution = coin_spend.solution.to_node_ptr(&mut allocator)?;

        let parse_ctx = parse_puzzle(&allocator, puzzle, solution, coin_spend.coin, nft_info.coin)?;
        let parse = parse_singleton(&allocator, &parse_ctx)?.unwrap();
        let parse = parse_nft(&mut allocator, &parse_ctx, &parse, u64::MAX)?;
        assert_eq!(
            parse.map(|nft_info| nft_info.with_metadata(())),
            Some(nft_info)
        );

        Ok(())
    }
}
