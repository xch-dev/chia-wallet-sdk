use std::sync::Arc;

use async_trait::async_trait;
use chia_bls::PublicKey;
use chia_client::{Error, Peer, PeerEvent};
use chia_protocol::{RegisterForPhUpdates, RespondToPhUpdates};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{DerivationState, KeyStore, Wallet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSettings {
    pub minimum_unused_derivations: u32,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            minimum_unused_derivations: 100,
        }
    }
}

#[async_trait]
pub trait DerivationWallet<S, K>: Wallet + Send + Sync
where
    S: DerivationState,
    K: KeyStore,
{
    fn calculate_puzzle_hash(&self, public_key: &PublicKey) -> [u8; 32];
    fn state(&self) -> &S;
    fn state_mut(&mut self) -> &mut S;
    fn key_store(&self) -> &Arc<Mutex<K>>;

    async fn fetch_unused_puzzle_hash(
        &mut self,
        peer: &Peer,
        sync_settings: &SyncSettings,
    ) -> Result<[u8; 32], Error> {
        let derivation_index = self
            .fetch_unused_derivation_index(peer, sync_settings)
            .await?;
        let public_key = self.key_store().lock().await.public_key(derivation_index);
        Ok(self.calculate_puzzle_hash(&public_key))
    }

    async fn fetch_unused_derivation_index(
        &mut self,
        peer: &Peer,
        sync_settings: &SyncSettings,
    ) -> Result<u32, Error> {
        // If there aren't any derivations, generate the first batch.
        if self.state().next_derivation_index().await == 0 {
            register_more_puzzle_hashes(self, peer, sync_settings.minimum_unused_derivations)
                .await?;
        }

        loop {
            let result = self.state().unused_derivation_index().await;
            if let Some(unused_index) = result {
                // Calculate the extra unused derivations after that index.
                let last_index = self.state().next_derivation_index().await - 1;
                let extra_indices = last_index - unused_index;

                // Make sure at least `gap` indices are available if needed.
                if extra_indices < sync_settings.minimum_unused_derivations {
                    register_more_puzzle_hashes(
                        self,
                        peer,
                        sync_settings.minimum_unused_derivations,
                    )
                    .await?;
                }

                // Return the unused derivation index.
                return Ok(unused_index);
            } else {
                // Generate more puzzle hashes and check again.
                register_more_puzzle_hashes(self, peer, sync_settings.minimum_unused_derivations)
                    .await?;
            }
        }
    }
}

pub async fn start_syncing<W, S, K>(
    wallet: Arc<Mutex<W>>,
    peer: Arc<Peer>,
    sync_settings: SyncSettings,
) -> Result<(), Error>
where
    W: DerivationWallet<S, K> + ?Sized,
    S: DerivationState,
    K: KeyStore,
{
    let mut event_receiver = peer.receiver().resubscribe();

    let mut lock = wallet.lock().await;

    let derivation_index = lock.state().next_derivation_index().await;
    if derivation_index > 0 {
        let mut derivations = Vec::new();
        for index in 0..derivation_index {
            let public_key = lock.key_store().lock().await.public_key(index);
            derivations.push(lock.calculate_puzzle_hash(&public_key));
        }

        lock.state_mut()
            .insert_next_derivations(derivations.clone())
            .await;

        let response: RespondToPhUpdates = peer
            .request(RegisterForPhUpdates::new(
                derivations
                    .into_iter()
                    .map(|derivation| derivation.into())
                    .collect(),
                0,
            ))
            .await?;

        lock.state_mut()
            .apply_state_updates(response.coin_states)
            .await;
    }

    lock.fetch_unused_derivation_index(&peer, &sync_settings)
        .await?;

    drop(lock);

    while let Ok(event) = event_receiver.recv().await {
        if let PeerEvent::CoinStateUpdate(update) = event {
            let mut lock = wallet.lock().await;
            lock.state_mut().apply_state_updates(update.items).await;
            lock.fetch_unused_derivation_index(&peer, &sync_settings)
                .await?;
        }
    }

    Ok(())
}

async fn register_more_puzzle_hashes<W, S, K>(
    wallet: &mut W,
    peer: &Peer,
    puzzle_hashes: u32,
) -> Result<Vec<[u8; 32]>, Error>
where
    W: DerivationWallet<S, K> + ?Sized,
    S: DerivationState,
    K: KeyStore,
{
    let next = wallet.state().next_derivation_index().await;
    let target = next + puzzle_hashes;
    wallet.key_store().lock().await.derive_keys_until(target);

    let mut derivations = Vec::new();
    for index in next..target {
        let public_key = wallet.key_store().lock().await.public_key(index);
        derivations.push(wallet.calculate_puzzle_hash(&public_key));
    }

    wallet
        .state_mut()
        .insert_next_derivations(derivations.clone())
        .await;

    let response: RespondToPhUpdates = peer
        .request(RegisterForPhUpdates::new(
            derivations
                .into_iter()
                .map(|derivation| derivation.into())
                .collect(),
            0,
        ))
        .await?;

    wallet
        .state_mut()
        .apply_state_updates(response.coin_states)
        .await;

    Ok(response
        .puzzle_hashes
        .into_iter()
        .map(|puzzle_hash| (&puzzle_hash).into())
        .collect())
}
