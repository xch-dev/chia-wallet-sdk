use chia_protocol::{Bytes32, CoinSpend};
use chia_puzzles::nft::{
    NftOwnershipLayerArgs, NftOwnershipLayerSolution, NftRoyaltyTransferPuzzleArgs,
    NftStateLayerArgs, NftStateLayerSolution,
};
use chia_sdk_types::{
    conditions::{Condition, NewNftOwner},
    puzzles::NftInfo,
};
use clvm_traits::{clvm_list, ToClvm, ToNodePtr};
use clvm_utils::{tree_hash, CurriedProgram};
use clvmr::{
    sha2::{Digest, Sha256},
    Allocator, NodePtr,
};

use crate::{spend_singleton, Conditions, Spend, SpendContext, SpendError};

#[derive(Debug, Clone)]
pub struct TransferNft<M> {
    pub did_conditions: Conditions,
    pub p2_conditions: Conditions,
    pub output: NftInfo<M>,
}

pub fn transfer_nft<M>(
    ctx: &mut SpendContext<'_>,
    nft_info: NftInfo<M>,
    p2_puzzle_hash: Bytes32,
    new_nft_owner: Option<NewNftOwner>,
) -> Result<TransferNft<M>, SpendError>
where
    M: ToClvm<NodePtr>,
{
    let mut did_conditions = Conditions::new();
    let mut p2_conditions =
        Conditions::new().create_hinted_coin(p2_puzzle_hash, nft_info.coin.amount, p2_puzzle_hash);

    let new_owner = if let Some(new_nft_owner) = new_nft_owner {
        did_conditions = did_conditions.assert_raw_puzzle_announcement(did_puzzle_assertion(
            nft_info.coin.puzzle_hash,
            &new_nft_owner,
        ));
        p2_conditions = p2_conditions.condition(Condition::Other(ctx.alloc(&new_nft_owner)?));
        new_nft_owner.did_id
    } else {
        nft_info.current_owner
    };

    Ok(TransferNft {
        did_conditions,
        p2_conditions,
        output: nft_info.child(p2_puzzle_hash, new_owner),
    })
}

#[allow(clippy::missing_panics_doc)]
pub fn did_puzzle_assertion(nft_full_puzzle_hash: Bytes32, new_nft_owner: &NewNftOwner) -> Bytes32 {
    let mut allocator = Allocator::new();

    let new_nft_owner_args = clvm_list!(
        new_nft_owner.did_id,
        &new_nft_owner.trade_prices,
        new_nft_owner.did_inner_puzzle_hash
    )
    .to_node_ptr(&mut allocator)
    .unwrap();

    let mut hasher = Sha256::new();
    hasher.update(nft_full_puzzle_hash);
    hasher.update([0xad, 0x4c]);
    hasher.update(tree_hash(&allocator, new_nft_owner_args));

    Bytes32::new(hasher.finalize().into())
}

pub fn nft_spend<M>(
    ctx: &mut SpendContext<'_>,
    nft_info: &NftInfo<M>,
    inner_spend: Spend,
) -> Result<CoinSpend, SpendError>
where
    M: ToClvm<NodePtr>,
{
    let transfer_program_puzzle = ctx.nft_royalty_transfer()?;

    let transfer_program = CurriedProgram {
        program: transfer_program_puzzle,
        args: NftRoyaltyTransferPuzzleArgs::new(
            nft_info.launcher_id,
            nft_info.royalty_puzzle_hash,
            nft_info.royalty_percentage,
        ),
    };

    let ownership_layer_spend =
        spend_nft_ownership_layer(ctx, nft_info.current_owner, transfer_program, inner_spend)?;

    let state_layer_spend = spend_nft_state_layer(ctx, &nft_info.metadata, ownership_layer_spend)?;

    spend_singleton(
        ctx,
        nft_info.coin,
        nft_info.launcher_id,
        nft_info.proof,
        state_layer_spend,
    )
}

pub fn spend_nft_state_layer<M>(
    ctx: &mut SpendContext<'_>,
    metadata: M,
    inner_spend: Spend,
) -> Result<Spend, SpendError>
where
    M: ToClvm<NodePtr>,
{
    let nft_state_layer = ctx.nft_state_layer()?;

    let puzzle = ctx.alloc(&CurriedProgram {
        program: nft_state_layer,
        args: NftStateLayerArgs::new(metadata, inner_spend.puzzle()),
    })?;

    let solution = ctx.alloc(&NftStateLayerSolution {
        inner_solution: inner_spend.solution(),
    })?;

    Ok(Spend::new(puzzle, solution))
}

pub fn spend_nft_ownership_layer<P>(
    ctx: &mut SpendContext<'_>,
    current_owner: Option<Bytes32>,
    transfer_program: P,
    inner_spend: Spend,
) -> Result<Spend, SpendError>
where
    P: ToClvm<NodePtr>,
{
    let nft_ownership_layer = ctx.nft_ownership_layer()?;

    let puzzle = ctx.alloc(&CurriedProgram {
        program: nft_ownership_layer,
        args: NftOwnershipLayerArgs::new(current_owner, transfer_program, inner_spend.puzzle()),
    })?;

    let solution = ctx.alloc(&NftOwnershipLayerSolution {
        inner_solution: inner_spend.solution(),
    })?;

    Ok(Spend::new(puzzle, solution))
}

#[cfg(test)]
mod tests {
    use crate::{nft_mint, IntermediateLauncher, Launcher};

    use super::*;

    use chia_bls::DerivableKey;
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{secret_key, test_transaction, Simulator};
    use clvmr::Allocator;

    #[tokio::test]
    async fn test_nft_transfer() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 2).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (create_did, did_info) = Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let (mint_nft, nft_info) = IntermediateLauncher::new(did_info.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, nft_mint(puzzle_hash, Some(&did_info)))?;

        let did_info = ctx.spend_standard_did(did_info, pk, mint_nft)?;

        let other_puzzle_hash = StandardArgs::curry_tree_hash(pk.derive_unhardened(0)).into();

        let (parent_conditions, _nft_info) =
            ctx.spend_standard_nft(&nft_info, pk, other_puzzle_hash, None, Conditions::new())?;

        let _did_info = ctx.spend_standard_did(did_info, pk, parent_conditions)?;

        test_transaction(
            &peer,
            ctx.take_spends(),
            &[sk],
            sim.config().genesis_challenge,
        )
        .await;

        Ok(())
    }

    #[tokio::test]
    async fn test_nft_lineage() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 2).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (create_did, did_info) = Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let (mint_nft, mut nft_info) = IntermediateLauncher::new(did_info.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, nft_mint(puzzle_hash, Some(&did_info)))?;

        let mut did_info = ctx.spend_standard_did(did_info, pk, mint_nft)?;

        for i in 0..5 {
            let (spend_nft, new_nft_info) = ctx.spend_standard_nft(
                &nft_info,
                pk,
                nft_info.p2_puzzle_hash,
                if i % 2 == 0 {
                    Some(NewNftOwner::new(
                        Some(did_info.launcher_id),
                        Vec::new(),
                        Some(did_info.inner_puzzle_hash),
                    ))
                } else {
                    None
                },
                Conditions::new(),
            )?;
            nft_info = new_nft_info;
            did_info = ctx.spend_standard_did(did_info, pk, spend_nft)?;
        }

        test_transaction(
            &peer,
            ctx.take_spends(),
            &[sk],
            sim.config().genesis_challenge,
        )
        .await;

        let coin_state = sim
            .coin_state(did_info.coin.coin_id())
            .await
            .expect("expected did coin");
        assert_eq!(coin_state.coin, did_info.coin);

        let coin_state = sim
            .coin_state(nft_info.coin.coin_id())
            .await
            .expect("expected nft coin");
        assert_eq!(coin_state.coin, nft_info.coin);

        Ok(())
    }
}
