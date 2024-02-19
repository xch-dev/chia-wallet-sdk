use std::{future::Future, sync::Arc};

use chia_client::{Peer, PeerEvent};
use chia_protocol::CoinState;
use tokio::sync::{broadcast, mpsc, Mutex};

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

/// An interface for everything needed to sync a wallet.
pub trait SyncManager {
    /// The error that may be returned from methods.
    type Error;

    /// Receives the next batch of coin state updates.
    fn receive_updates(&self) -> impl Future<Output = Option<Vec<CoinState>>> + Send;

    /// Subscribes to a set of puzzle hashes and returns the initial coin states.
    fn subscribe(
        &self,
        puzzle_hashes: Vec<[u8; 32]>,
        min_height: u32,
    ) -> impl Future<Output = Result<Vec<CoinState>, Self::Error>> + Send;

    /// Whether or not a given puzzle hash has been used.
    fn is_used(&self, puzzle_hash: [u8; 32]) -> impl Future<Output = bool> + Send;

    /// Sent whenever the wallet has been caught up.
    fn handle_synced(&self) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Sent whenever a coin which does not match a puzzle hash directly is received.
    fn apply_updates(
        &self,
        coin_states: Vec<CoinState>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// A simple implementation of a sync manager, that syncs against a single peer and coin store.
pub struct SimpleSyncManager<C> {
    peer: Arc<Peer>,
    receiver: Mutex<broadcast::Receiver<PeerEvent>>,
    sender: mpsc::Sender<()>,
    coin_store: Arc<C>,
}

impl<C> SimpleSyncManager<C> {
    /// Creates a new sync manager for a given peer and coin store.
    /// The sender is for whenever the wallet is synced.
    pub fn new(peer: Arc<Peer>, coin_store: Arc<C>, sender: mpsc::Sender<()>) -> Self {
        let receiver = peer.receiver().resubscribe();

        Self {
            peer,
            receiver: Mutex::new(receiver),
            sender,
            coin_store,
        }
    }
}

impl<C> SyncManager for SimpleSyncManager<C>
where
    C: CoinStore + Send + Sync,
{
    type Error = chia_client::Error<()>;

    async fn receive_updates(&self) -> Option<Vec<CoinState>> {
        loop {
            if let PeerEvent::CoinStateUpdate(update) =
                self.receiver.lock().await.recv().await.ok()?
            {
                return Some(update.items);
            }
        }
    }

    async fn subscribe(
        &self,
        puzzle_hashes: Vec<[u8; 32]>,
        min_height: u32,
    ) -> Result<Vec<CoinState>, Self::Error> {
        self.peer
            .register_for_ph_updates(
                puzzle_hashes.into_iter().map(Into::into).collect(),
                min_height,
            )
            .await
    }

    async fn is_used(&self, puzzle_hash: [u8; 32]) -> bool {
        self.coin_store.is_used(puzzle_hash).await
    }

    async fn handle_synced(&self) -> Result<(), Self::Error> {
        self.sender.send(()).await.unwrap();
        Ok(())
    }

    async fn apply_updates(&self, coin_states: Vec<CoinState>) -> Result<(), Self::Error> {
        self.coin_store.update_coin_state(coin_states).await;
        Ok(())
    }
}

/// Syncs a derivation wallet.
pub async fn incremental_sync<Err>(
    sync_manager: Arc<impl SyncManager<Error = Err>>,
    derivation_store: Arc<impl DerivationStore>,
    config: SyncConfig,
) -> Result<(), Err> {
    let derivations = derivation_store.count().await;

    if derivations > 0 {
        let mut puzzle_hashes = Vec::new();
        for index in 0..derivations {
            puzzle_hashes.push(derivation_store.puzzle_hash(index).await.unwrap());
        }
        let coin_states = sync_manager.subscribe(puzzle_hashes, 0).await?;
        sync_manager.apply_updates(coin_states).await?;
    }

    sync_to_unused_index(sync_manager.as_ref(), derivation_store.as_ref(), &config).await?;

    sync_manager.handle_synced().await?;

    while let Some(updates) = sync_manager.receive_updates().await {
        sync_manager.apply_updates(updates).await?;
        sync_to_unused_index(sync_manager.as_ref(), derivation_store.as_ref(), &config).await?;

        sync_manager.handle_synced().await?;
    }

    Ok(())
}

/// Subscribe to another set of puzzle hashes.
pub async fn subscribe<Err>(
    sync_manager: &impl SyncManager<Error = Err>,
    puzzle_hashes: Vec<[u8; 32]>,
) -> Result<(), Err> {
    let mut i = 0;
    while i < puzzle_hashes.len() {
        let coin_states = sync_manager
            .subscribe(puzzle_hashes[i..i + 100].to_vec(), 0)
            .await?;
        sync_manager.apply_updates(coin_states).await?;
        i += 100;
    }
    Ok(())
}

/// Create more derivations for a wallet.
pub async fn derive_more<Err>(
    sync_manager: &impl SyncManager<Error = Err>,
    derivation_store: &impl DerivationStore,
    amount: u32,
) -> Result<(), Err> {
    let start = derivation_store.count().await;
    derivation_store.derive_to_index(start + amount).await;

    let mut puzzle_hashes: Vec<[u8; 32]> = Vec::new();

    for index in start..(start + amount) {
        puzzle_hashes.push(derivation_store.puzzle_hash(index).await.unwrap());
    }

    subscribe(sync_manager, puzzle_hashes).await
}

/// Gets the last unused derivation index for a wallet.
pub async fn unused_index<Err>(
    sync_manager: &impl SyncManager<Error = Err>,
    derivation_store: &impl DerivationStore,
) -> Result<Option<u32>, Err> {
    let derivations = derivation_store.count().await;
    let mut unused_index = None;
    for index in (0..derivations).rev() {
        if !sync_manager
            .is_used(derivation_store.puzzle_hash(index).await.unwrap())
            .await
        {
            unused_index = Some(index);
        } else {
            break;
        }
    }
    Ok(unused_index)
}

/// Syncs a wallet such that there are enough unused derivations.
pub async fn sync_to_unused_index<Err>(
    sync_manager: &impl SyncManager<Error = Err>,
    derivation_store: &impl DerivationStore,
    config: &SyncConfig,
) -> Result<u32, Err> {
    // If there aren't any derivations, generate the first batch.
    let derivations = derivation_store.count().await;

    if derivations == 0 {
        derive_more(
            sync_manager,
            derivation_store,
            config.minimum_unused_derivations,
        )
        .await?;
    }

    loop {
        let derivations = derivation_store.count().await;
        let result = unused_index(sync_manager, derivation_store).await?;

        if let Some(unused_index) = result {
            // Calculate the extra unused derivations after that index.
            let extra_indices = derivations - unused_index;

            // Make sure at least `gap` indices are available if needed.
            if extra_indices < config.minimum_unused_derivations {
                derive_more(
                    sync_manager,
                    derivation_store,
                    config.minimum_unused_derivations,
                )
                .await?;
            }

            // Return the unused derivation index.
            return Ok(unused_index);
        }

        // Generate more puzzle hashes and check again.
        derive_more(
            sync_manager,
            derivation_store,
            config.minimum_unused_derivations,
        )
        .await?;
    }
}
