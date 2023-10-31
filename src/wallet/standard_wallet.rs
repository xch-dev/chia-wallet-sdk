use std::sync::Arc;

use crate::{KeyStore, Wallet};

mod state;

use chia_client::{Peer, PeerEvent};
use chia_protocol::{RegisterForPhUpdates, RespondToPhUpdates};
use chia_wallet::standard::standard_puzzle_hash;
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
    fn spendable_balance(&self) -> u64 {
        self.state
            .lock()
            .spendable_coins()
            .iter()
            .fold(0, |balance, coin_state| balance + coin_state.coin.amount)
    }
}

impl<K, S> StandardWallet<K, S>
where
    K: KeyStore + 'static,
    S: StandardState + 'static,
{
    pub fn new(key_store: Arc<K>, peer: Arc<Peer>, state: Arc<Mutex<S>>) -> Self {
        let key_store_clone = Arc::clone(&key_store);
        let peer_clone = Arc::clone(&peer);
        let state_clone = Arc::clone(&state);

        tokio::spawn(async move {
            let puzzle_hash = standard_puzzle_hash(&key_store_clone.public_key(0));

            let body = RegisterForPhUpdates::new(vec![puzzle_hash.into()], 0);
            let response = peer_clone.request::<_, RespondToPhUpdates>(body).await;

            match response {
                Ok(response) => {
                    state_clone.lock().update_coin_states(response.coin_states);
                }
                Err(error) => {
                    log::error!("could not register for puzzle hash updates: {error}");
                }
            }

            let event_receiver = peer_clone.receiver().resubscribe();
            Self::handle_events(event_receiver, state_clone).await;
        });

        Self {
            key_store,
            peer,
            state,
        }
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
