mod create_did;
mod did_info;
mod did_spend;

pub use create_did::*;
pub use did_info::*;
pub use did_spend::*;

#[cfg(test)]
mod tests {
    use super::*;

    use clvmr::Allocator;

    use crate::{test::TestWallet, Chainable, Launcher, StandardSpend};

    #[tokio::test]
    async fn test_create_did() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let mut wallet = TestWallet::new(&mut allocator, 1).await;
        let ctx = &mut wallet.ctx;

        let (launch_singleton, _did_info) = Launcher::new(wallet.coin.coin_id(), 1)
            .create(ctx)?
            .create_standard_did(ctx, wallet.pk)?;

        StandardSpend::new()
            .chain(launch_singleton)
            .finish(ctx, wallet.coin, wallet.pk)?;

        wallet.submit().await?;

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
