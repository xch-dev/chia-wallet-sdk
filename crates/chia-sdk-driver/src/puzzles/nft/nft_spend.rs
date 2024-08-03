#[cfg(test)]
mod tests {
    use crate::{nft_mint, IntermediateLauncher, Launcher};

    use super::*;

    use chia_bls::DerivableKey;
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{secret_key, test_transaction, Simulator};

    #[tokio::test]
    async fn test_nft_transfer() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 2).await;

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
        let ctx = &mut SpendContext::new();

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 2).await;

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
