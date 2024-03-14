use std::future::Future;

use chia_protocol::{Coin, CoinState};

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
