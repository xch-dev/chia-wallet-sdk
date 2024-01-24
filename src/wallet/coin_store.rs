use std::{collections::HashMap, future::Future};

use chia_protocol::{Coin, CoinState};
use parking_lot::Mutex;

/// Keeps track of the state of coins in a wallet.
pub trait CoinStore {
    /// Applies coin state updates.
    fn update_coin_state(&self, coin_states: Vec<CoinState>) -> impl Future<Output = ()> + Send;

    /// Gets a list of unspent coins.
    fn unspent_coins(&self) -> impl Future<Output = Vec<Coin>> + Send;

    /// Gets the current state of a coin.
    fn coin_state(&self, coin_id: [u8; 32]) -> impl Future<Output = Option<CoinState>> + Send;

    /// Gets coin states for a given puzzle hash.
    fn is_used(&self, puzzle_hash: [u8; 32]) -> impl Future<Output = bool> + Send;
}

/// An in-memory coin store implementation.
#[derive(Default)]
pub struct MemoryCoinStore {
    // These are keyed by puzzle hash for performance.
    coin_states: Mutex<HashMap<[u8; 32], Vec<CoinState>>>,
}

impl MemoryCoinStore {
    /// Creates a new in-memory coin store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl CoinStore for MemoryCoinStore {
    async fn update_coin_state(&self, coin_states: Vec<CoinState>) {
        for coin_state in coin_states {
            let puzzle_hash = &coin_state.coin.puzzle_hash;
            let puzzle_hash = <&[u8; 32]>::from(puzzle_hash);
            let mut db = self.coin_states.lock();

            if let Some(items) = db.get_mut(puzzle_hash) {
                match items.iter_mut().find(|item| item.coin == coin_state.coin) {
                    Some(value) => {
                        *value = coin_state;
                    }
                    None => items.push(coin_state),
                }
            } else {
                db.insert(*puzzle_hash, vec![coin_state]);
            }
        }
    }

    async fn unspent_coins(&self) -> Vec<Coin> {
        self.coin_states
            .lock()
            .values()
            .flatten()
            .filter(|coin_state| coin_state.spent_height.is_none())
            .map(|coin_state| coin_state.coin.clone())
            .collect()
    }

    async fn coin_state(&self, coin_id: [u8; 32]) -> Option<CoinState> {
        self.coin_states
            .lock()
            .values()
            .flatten()
            .find(|coin_state| coin_state.coin.coin_id() == coin_id)
            .cloned()
    }

    async fn is_used(&self, puzzle_hash: [u8; 32]) -> bool {
        self.coin_states
            .lock()
            .get(&puzzle_hash)
            .map(|coin_states| !coin_states.is_empty())
            .unwrap_or_default()
    }
}
