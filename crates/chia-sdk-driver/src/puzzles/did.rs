mod create_did;
mod did_spend;

pub use create_did::*;
pub use did_spend::*;

#[cfg(test)]
mod tests {
    use crate::{
        puzzles::{Launcher, StandardSpend},
        spend_builder::Chainable,
        SpendContext,
    };

    use super::*;

    use chia_sdk_test::TestWallet;
    use clvmr::Allocator;

    #[tokio::test]
    async fn test_create_did() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);
        let mut wallet = TestWallet::new(1).await;

        let (launch_singleton, _did_info) = Launcher::new(wallet.coin.coin_id(), 1)
            .create(ctx)?
            .create_standard_did(ctx, wallet.pk)?;

        StandardSpend::new()
            .chain(launch_singleton)
            .finish(ctx, wallet.coin, wallet.pk)?;

        wallet.submit(ctx.take_spends()).await?;

        // Make sure the DID was created.
        let found_coins = wallet
            .peer
            .register_for_ph_updates(vec![wallet.puzzle_hash], 0)
            .await
            .unwrap();
        assert_eq!(found_coins.len(), 2);

        Ok(())
    }
}
