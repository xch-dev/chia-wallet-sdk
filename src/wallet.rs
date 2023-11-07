use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chia_client::{Peer, PeerEvent};
use chia_protocol::{Coin, RegisterForPhUpdates, RespondToPhUpdates};
use chia_wallet::{
    standard::{standard_puzzle_hash, DEFAULT_HIDDEN_PUZZLE_HASH},
    DeriveSynthetic,
};
use parking_lot::Mutex;
use tokio::task::JoinHandle;

use crate::KeyStore;

mod derivation_info;
mod standard_state;

pub use derivation_info::*;
pub use standard_state::*;

pub trait Wallet {
    fn spendable_coins(&self) -> Vec<Coin>;

    fn spendable_balance(&self) -> u64 {
        self.spendable_coins()
            .iter()
            .fold(0, |balance, coin| balance + coin.amount)
    }
}

#[async_trait]
pub trait DerivationWallet {
    fn derivation_index(&self, puzzle_hash: [u8; 32]) -> Option<u32>;
    fn unused_derivation_index(&self) -> Option<u32>;
    fn next_derivation_index(&self) -> u32;

    async fn generate_puzzle_hashes(&self, puzzle_hashes: u32) -> Result<Vec<[u8; 32]>>;

    async fn sync(&self, gap: u32) -> Result<u32> {
        // If there aren't any derivations, generate the first batch.
        if self.next_derivation_index() == 0 {
            self.generate_puzzle_hashes(gap).await?;
        }

        loop {
            match self.unused_derivation_index() {
                // Check if an unused derivation index was found.
                Some(unused_index) => {
                    // If so, calculate the extra unused derivations after that index.
                    let last_index = self.next_derivation_index() - 1;
                    let extra_indices = last_index - unused_index;

                    // Make sure at least `gap` indices are available if needed.
                    if extra_indices < gap {
                        self.generate_puzzle_hashes(gap).await?;
                    }

                    // Return the unused derivation index.
                    return Ok(unused_index);
                }
                // Otherwise, generate more puzzle hashes and check again.
                None => {
                    self.generate_puzzle_hashes(gap).await?;
                }
            }
        }
    }
}

pub struct StandardWallet<S>
where
    S: StandardState,
{
    key_store: Arc<KeyStore>,
    peer: Arc<Peer>,
    state: Arc<Mutex<S>>,
    join_handle: Option<JoinHandle<()>>,
}

impl<S> Wallet for StandardWallet<S>
where
    S: StandardState,
{
    fn spendable_coins(&self) -> Vec<Coin> {
        self.state.lock().spendable_coins()
    }
}

#[async_trait]
impl<S> DerivationWallet for StandardWallet<S>
where
    S: StandardState + 'static,
{
    fn derivation_index(&self, puzzle_hash: [u8; 32]) -> Option<u32> {
        self.state.lock().derivation_index(puzzle_hash)
    }

    fn unused_derivation_index(&self) -> Option<u32> {
        self.state.lock().unused_derivation_index()
    }

    fn next_derivation_index(&self) -> u32 {
        self.state.lock().next_derivation_index()
    }

    async fn generate_puzzle_hashes(&self, puzzle_hashes: u32) -> Result<Vec<[u8; 32]>> {
        let derivation_index = self.next_derivation_index();

        let derivations = (derivation_index..derivation_index + puzzle_hashes).map(|index| {
            let public_key = self.key_store.public_key(index);
            let synthetic_pk = public_key.derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);
            let puzzle_hash = standard_puzzle_hash(&synthetic_pk);
            DerivationInfo {
                puzzle_hash,
                synthetic_pk,
            }
        });

        self.state
            .lock()
            .insert_next_derivations(derivations.clone());

        let response: RespondToPhUpdates = self
            .peer
            .request(RegisterForPhUpdates::new(
                derivations
                    .map(|derivation| derivation.puzzle_hash.into())
                    .collect(),
                0,
            ))
            .await?;

        self.state.lock().apply_state_updates(response.coin_states);

        Ok(response
            .puzzle_hashes
            .into_iter()
            .map(|puzzle_hash| (&puzzle_hash).into())
            .collect())
    }
}

impl<S> StandardWallet<S>
where
    S: StandardState + 'static,
{
    pub fn new(key_store: Arc<KeyStore>, peer: Arc<Peer>, state: S, gap: u32) -> Self {
        let mut event_receiver = peer.receiver().resubscribe();
        let state = Arc::new(Mutex::new(state));

        let wallet = Self {
            key_store: key_store.clone(),
            peer: peer.clone(),
            state: state.clone(),
            join_handle: None,
        };

        let join_handle = tokio::spawn(async move {
            if let Err(error) = wallet.sync(gap).await {
                log::error!("failed to perform initial wallet sync: {error}");
            }

            while let Ok(event) = event_receiver.recv().await {
                if let PeerEvent::CoinStateUpdate(update) = event {
                    wallet.state.lock().apply_state_updates(update.items);
                    if let Err(error) = wallet.sync(gap).await {
                        log::error!("failed to sync wallet after coin state update: {error}");
                    }
                }
            }
        });

        Self {
            key_store,
            peer,
            state,
            join_handle: Some(join_handle),
        }
    }
}

impl<S> Drop for StandardWallet<S>
where
    S: StandardState,
{
    fn drop(&mut self) {
        if let Some(join_handle) = self.join_handle.take() {
            join_handle.abort();
        }
    }
}
