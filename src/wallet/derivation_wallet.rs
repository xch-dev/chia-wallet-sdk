use std::sync::Arc;

use anyhow::Result;
use chia_bls::PublicKey;
use chia_client::{Peer, PeerEvent};
use chia_protocol::{Coin, RegisterForPhUpdates, RespondToPhUpdates};
use parking_lot::Mutex;
use tokio::task::JoinHandle;

use crate::{DerivationState, KeyStore, PuzzleGenerator, Wallet};

pub struct DerivationWallet<P, K, S>
where
    P: PuzzleGenerator,
    K: KeyStore,
    S: DerivationState,
{
    puzzle_generator: P,
    key_store: Arc<Mutex<K>>,
    peer: Arc<Peer>,
    state: Arc<Mutex<S>>,
    join_handle: Option<JoinHandle<()>>,
}

impl<P, K, S> DerivationWallet<P, K, S>
where
    P: PuzzleGenerator + Clone + 'static,
    K: KeyStore + 'static,
    S: DerivationState + 'static,
{
    pub fn new(
        puzzle_generator: P,
        key_store: Arc<Mutex<K>>,
        peer: Arc<Peer>,
        state: S,
        gap: u32,
    ) -> Self {
        let mut event_receiver = peer.receiver().resubscribe();
        let state = Arc::new(Mutex::new(state));

        let wallet = Self {
            puzzle_generator: puzzle_generator.clone(),
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
            puzzle_generator,
            key_store,
            peer,
            state,
            join_handle: Some(join_handle),
        }
    }

    pub fn peer(&self) -> &Peer {
        &self.peer
    }

    pub fn puzzle_generator(&self) -> &P {
        &self.puzzle_generator
    }

    async fn register_puzzle_hashes(&self, puzzle_hashes: u32) -> Result<Vec<[u8; 32]>> {
        let next = self.next_derivation_index();
        let target = next + puzzle_hashes;
        self.key_store.lock().derive_keys_until(target);

        let derivations = (next..target).map(|index| {
            let public_key = self.key_store.lock().public_key(index);
            self.puzzle_generator.puzzle_hash(&public_key)
        });

        self.state
            .lock()
            .insert_next_derivations(derivations.clone());

        let response: RespondToPhUpdates = self
            .peer
            .request(RegisterForPhUpdates::new(
                derivations.map(|derivation| derivation.into()).collect(),
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

    async fn sync(&self, gap: u32) -> Result<u32> {
        // If there aren't any derivations, generate the first batch.
        if self.next_derivation_index() == 0 {
            self.register_puzzle_hashes(gap).await?;
        }

        loop {
            if let Some(unused_index) = self.unused_derivation_index() {
                // Calculate the extra unused derivations after that index.
                let last_index = self.next_derivation_index() - 1;
                let extra_indices = last_index - unused_index;

                // Make sure at least `gap` indices are available if needed.
                if extra_indices < gap {
                    self.register_puzzle_hashes(gap).await?;
                }

                // Return the unused derivation index.
                return Ok(unused_index);
            } else {
                // Generate more puzzle hashes and check again.
                self.register_puzzle_hashes(gap).await?;
            }
        }
    }

    pub fn public_key(&self, index: u32) -> PublicKey {
        self.key_store.lock().public_key(index)
    }

    pub fn derivation_index(&self, puzzle_hash: [u8; 32]) -> Option<u32> {
        self.state.lock().derivation_index(puzzle_hash)
    }

    pub fn unused_derivation_index(&self) -> Option<u32> {
        self.state.lock().unused_derivation_index()
    }

    pub fn next_derivation_index(&self) -> u32 {
        self.state.lock().next_derivation_index()
    }
}

impl<P, K, S> Wallet for DerivationWallet<P, K, S>
where
    P: PuzzleGenerator,
    K: KeyStore,
    S: DerivationState,
{
    fn spendable_coins(&self) -> Vec<Coin> {
        self.state.lock().spendable_coins()
    }
}

impl<P, K, S> Drop for DerivationWallet<P, K, S>
where
    P: PuzzleGenerator,
    K: KeyStore,
    S: DerivationState,
{
    fn drop(&mut self) {
        if let Some(join_handle) = self.join_handle.take() {
            join_handle.abort();
        }
    }
}
