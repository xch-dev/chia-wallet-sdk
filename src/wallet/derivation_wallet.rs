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
pub trait DerivationWallet<S, K>: Wallet
where
    S: DerivationState,
    K: KeyStore,
{
    fn calculate_puzzle_hash(&self, public_key: &PublicKey) -> [u8; 32];
    fn state(&self) -> &Arc<Mutex<S>>;
    fn key_store(&self) -> &Arc<Mutex<K>>;
    fn peer(&self) -> &Arc<Peer>;

    async fn fetch_unused_puzzle_hash(
        &self,
        sync_settings: &SyncSettings,
    ) -> Result<[u8; 32], Error> {
        let derivation_index = self.fetch_unused_derivation_index(sync_settings).await?;
        let public_key = self.key_store().lock().await.public_key(derivation_index);
        Ok(self.calculate_puzzle_hash(&public_key))
    }

    async fn keep_synced_automatically(&self, sync_settings: SyncSettings) {
        let mut event_receiver = self.peer().receiver().resubscribe();

        if let Err(error) = self.fetch_unused_derivation_index(&sync_settings).await {
            log::error!("failed to perform initial wallet sync: {error}");
        }

        while let Ok(event) = event_receiver.recv().await {
            if let PeerEvent::CoinStateUpdate(update) = event {
                self.state()
                    .lock()
                    .await
                    .apply_state_updates(update.items)
                    .await;

                if let Err(error) = self.fetch_unused_derivation_index(&sync_settings).await {
                    log::error!("failed to sync wallet after coin state update: {error}");
                }
            }
        }
    }

    async fn fetch_unused_derivation_index(
        &self,
        sync_settings: &SyncSettings,
    ) -> Result<u32, Error> {
        // If there aren't any derivations, generate the first batch.
        if self.state().lock().await.next_derivation_index().await == 0 {
            self.register_puzzle_hashes(sync_settings.minimum_unused_derivations)
                .await?;
        }

        loop {
            let result = self.state().lock().await.unused_derivation_index().await;
            if let Some(unused_index) = result {
                // Calculate the extra unused derivations after that index.
                let last_index = self.state().lock().await.next_derivation_index().await - 1;
                let extra_indices = last_index - unused_index;

                // Make sure at least `gap` indices are available if needed.
                if extra_indices < sync_settings.minimum_unused_derivations {
                    self.register_puzzle_hashes(sync_settings.minimum_unused_derivations)
                        .await?;
                }

                // Return the unused derivation index.
                return Ok(unused_index);
            } else {
                // Generate more puzzle hashes and check again.
                self.register_puzzle_hashes(sync_settings.minimum_unused_derivations)
                    .await?;
            }
        }
    }

    async fn register_puzzle_hashes(&self, puzzle_hashes: u32) -> Result<Vec<[u8; 32]>, Error> {
        let next = self.state().lock().await.next_derivation_index().await;
        let target = next + puzzle_hashes;
        let public_keys = self.key_store().lock().await.derive_keys_until(target);

        let derivations: Vec<[u8; 32]> = public_keys
            .iter()
            .map(|public_key| self.calculate_puzzle_hash(public_key))
            .collect();

        self.state()
            .lock()
            .await
            .insert_next_derivations(derivations.clone())
            .await;

        let response: RespondToPhUpdates = self
            .peer()
            .request(RegisterForPhUpdates::new(
                derivations
                    .into_iter()
                    .map(|derivation| derivation.into())
                    .collect(),
                0,
            ))
            .await?;

        self.state()
            .lock()
            .await
            .apply_state_updates(response.coin_states)
            .await;

        Ok(response
            .puzzle_hashes
            .into_iter()
            .map(|puzzle_hash| (&puzzle_hash).into())
            .collect())
    }
}
