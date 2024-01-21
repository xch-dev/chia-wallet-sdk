use std::sync::Arc;

use chia_client::{Error, Peer, PeerEvent};

use crate::{CoinStore, DerivationStore};

/// Settings used while syncing a derivation wallet.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// The minimum number of unused derivation indices.
    pub minimum_unused_derivations: u32,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            minimum_unused_derivations: 100,
        }
    }
}

/// Syncs a derivation wallet.
pub async fn incremental_sync(
    peer: Arc<Peer>,
    derivation_store: Arc<impl DerivationStore>,
    coin_store: Arc<impl CoinStore>,
    config: SyncConfig,
) -> Result<(), Error<()>> {
    let mut event_receiver = peer.receiver().resubscribe();

    let derivations = derivation_store.derivations().await;

    if derivations > 0 {
        let mut puzzle_hashes = Vec::new();
        for index in 0..derivations {
            puzzle_hashes.push(derivation_store.puzzle_hash(index).await.unwrap().into());
        }
        let coin_states = peer.register_for_ph_updates(puzzle_hashes, 0).await?;
        coin_store.update_coin_state(coin_states).await;
    }

    sync_to_unused_index(
        peer.as_ref(),
        derivation_store.as_ref(),
        coin_store.as_ref(),
        &config,
    )
    .await?;

    while let Ok(event) = event_receiver.recv().await {
        if let PeerEvent::CoinStateUpdate(update) = event {
            coin_store.update_coin_state(update.items).await;
            sync_to_unused_index(
                peer.as_ref(),
                derivation_store.as_ref(),
                coin_store.as_ref(),
                &config,
            )
            .await?;
        }
    }

    Ok(())
}

async fn subscribe(
    peer: &Peer,
    coin_store: &impl CoinStore,
    puzzle_hashes: Vec<[u8; 32]>,
) -> Result<(), Error<()>> {
    let coin_states = peer
        .register_for_ph_updates(puzzle_hashes.into_iter().map(|ph| ph.into()).collect(), 0)
        .await?;
    coin_store.update_coin_state(coin_states).await;
    Ok(())
}

async fn derive_more(
    peer: &Peer,
    derivation_store: &impl DerivationStore,
    coin_store: &impl CoinStore,
    amount: u32,
) -> Result<(), Error<()>> {
    let start = derivation_store.derivations().await;
    derivation_store.derive_to_index(start + amount).await;

    let mut puzzle_hashes: Vec<[u8; 32]> = Vec::new();

    for index in start..(start + amount) {
        puzzle_hashes.push(derivation_store.puzzle_hash(index).await.unwrap());
    }

    subscribe(peer, coin_store, puzzle_hashes).await
}

async fn unused_index(
    derivation_store: &impl DerivationStore,
    coin_store: &impl CoinStore,
) -> Option<u32> {
    let derivations = derivation_store.derivations().await;
    for index in (0..derivations).rev() {
        let puzzle_hash = derivation_store.puzzle_hash(index).await.unwrap();
        if !coin_store.is_used(puzzle_hash).await {
            return Some(index);
        }
    }
    None
}

async fn sync_to_unused_index(
    peer: &Peer,
    derivation_store: &impl DerivationStore,
    coin_store: &impl CoinStore,
    config: &SyncConfig,
) -> Result<u32, Error<()>> {
    // If there aren't any derivations, generate the first batch.
    let derivations = derivation_store.derivations().await;

    if derivations == 0 {
        derive_more(
            peer,
            derivation_store,
            coin_store,
            config.minimum_unused_derivations,
        )
        .await?;
    }

    loop {
        let derivations = derivation_store.derivations().await;
        let result = unused_index(derivation_store, coin_store).await;

        if let Some(unused_index) = result {
            // Calculate the extra unused derivations after that index.
            let extra_indices = derivations - 1 - unused_index;

            // Make sure at least `gap` indices are available if needed.
            if extra_indices < config.minimum_unused_derivations {
                derive_more(
                    peer,
                    derivation_store,
                    coin_store,
                    config.minimum_unused_derivations,
                )
                .await?;
            }

            // Return the unused derivation index.
            return Ok(unused_index);
        } else {
            // Generate more puzzle hashes and check again.
            derive_more(
                peer,
                derivation_store,
                coin_store,
                config.minimum_unused_derivations,
            )
            .await?;
        }
    }
}
