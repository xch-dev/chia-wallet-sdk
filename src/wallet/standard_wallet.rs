use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{KeyStore, Wallet};

mod state;

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
    K: KeyStore,
    S: StandardState,
{
    fn spendable_balance(&self) -> u64 {
        0
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
