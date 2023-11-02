use std::sync::Arc;

use crate::{KeyStore, Wallet};

mod state;

use chia_client::{Peer, PeerEvent};
use chia_protocol::{BytesImpl, Coin, RegisterForPhUpdates, RespondToPhUpdates};
use chia_wallet::{
    standard::{standard_puzzle_hash, DEFAULT_HIDDEN_PUZZLE_HASH},
    DeriveSynthetic,
};
use itertools::Itertools;
use parking_lot::Mutex;
pub use state::*;
use tokio::sync::broadcast;

pub struct StandardWallet<K, S>
where
    K: KeyStore,
    S: StandardState,
{
    key_store: Arc<K>,
    peer: Arc<Peer>,
    state: Arc<Mutex<S>>,
}

impl<K, S> Wallet for StandardWallet<K, S>
where
    K: KeyStore,
    S: StandardState,
{
    fn spendable_coins(&self) -> Vec<Coin> {
        self.state.lock().spendable_coins()
    }
}

impl<K, S> StandardWallet<K, S>
where
    K: KeyStore + 'static,
    S: StandardState + 'static,
{
    pub fn new(key_store: Arc<K>, peer: Arc<Peer>, state: S) -> Self {
        let state = Arc::new(Mutex::new(state));

        let event_receiver = peer.receiver().resubscribe();
        let state_clone = Arc::clone(&state);

        tokio::spawn(async move {
            Self::handle_events(event_receiver, state_clone).await;
        });

        Self {
            key_store,
            peer,
            state,
        }
    }

    pub async fn sync(&self) {
        match self.more_puzzle_hashes().await {
            Ok(response) => self.state.lock().update_coin_states(response.coin_states),
            Err(error) => log::error!("could not register for puzzle hash updates: {error}"),
        }
    }

    async fn more_puzzle_hashes(&self) -> chia_client::Result<RespondToPhUpdates> {
        let puzzle_hashes = (0..100)
            .map(|index| {
                let public_key = self.key_store.public_key(index);
                let synthetic_key = public_key.derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);
                BytesImpl::from(standard_puzzle_hash(&synthetic_key))
            })
            .collect_vec();

        self.peer
            .request(RegisterForPhUpdates::new(puzzle_hashes, 0))
            .await
    }

    async fn handle_events(
        mut event_receiver: broadcast::Receiver<PeerEvent>,
        state: Arc<Mutex<S>>,
    ) {
        while let Ok(event) = event_receiver.recv().await {
            if let PeerEvent::CoinStateUpdate(update) = event {
                state.lock().update_coin_states(update.items);
            }
        }
    }
}
