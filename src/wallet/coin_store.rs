use std::{collections::HashMap, future::Future};

use chia_protocol::CoinState;
use parking_lot::Mutex;

/// Keeps track of the state of coins in a wallet.
pub trait CoinStore {
    /// Applies coin state updates.
    fn update_coin_state(&self, coin_states: Vec<CoinState>) -> impl Future<Output = ()> + Send;

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

            if let Some(items) = self
                .coin_states
                .lock()
                .get_mut(<&[u8; 32]>::from(puzzle_hash))
            {
                match items.iter_mut().find(|item| item.coin == coin_state.coin) {
                    Some(value) => {
                        *value = coin_state;
                    }
                    None => items.push(coin_state),
                }
            }
        }
    }

    async fn is_used(&self, puzzle_hash: [u8; 32]) -> bool {
        self.coin_states
            .lock()
            .get(&puzzle_hash)
            .map(|coin_states| !coin_states.is_empty())
            .unwrap_or_default()
    }
}
