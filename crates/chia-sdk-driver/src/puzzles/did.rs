mod create_did;
mod did_spend;

pub use create_did::*;
pub use did_spend::*;

#[cfg(test)]
mod tests {
    use crate::{Launcher, SpendContext};

    use super::*;

    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{test_transaction, Simulator};
    use clvmr::Allocator;

    #[tokio::test]
    async fn test_create_did() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = sim.secret_key().await?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 1).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (launch_singleton, did_info) =
            Launcher::new(coin.coin_id(), 1).create_standard_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, launch_singleton)?;

        test_transaction(
            &peer,
            ctx.take_spends(),
            &[sk],
            sim.config().genesis_challenge,
        )
        .await;

        // Make sure the DID was created.
        let coin_state = sim
            .coin_state(did_info.coin.coin_id())
            .await
            .expect("expected did coin");
        assert_eq!(coin_state.coin, did_info.coin);

        Ok(())
    }
}
