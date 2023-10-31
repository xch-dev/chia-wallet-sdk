use std::sync::Arc;

use crate::{KeyStore, Wallet};

mod state;

use parking_lot::Mutex;
pub use state::*;

pub struct StandardWallet<K, S>
where
    K: KeyStore,
    S: StandardState,
{
    key_store: Arc<Mutex<K>>,
    state: Arc<Mutex<S>>,
}

impl<K, S> Wallet for StandardWallet<K, S>
where
    K: KeyStore + Send,
    S: StandardState + Send,
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
    K: KeyStore,
    S: StandardState,
{
    pub fn new(key_store: Arc<Mutex<K>>, state: S) -> Self {
        Self {
            key_store,
            state: Arc::new(Mutex::new(state)),
        }
    }
}
