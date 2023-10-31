use std::sync::Arc;

use chia_client::Peer;

use crate::KeyStore;

pub struct Wallet<K>
where
    K: KeyStore,
{
    peer: Arc<Peer>,
    key_store: K,
}

impl<K> Wallet<K>
where
    K: KeyStore,
{
    pub fn new(peer: Arc<Peer>, key_store: K) -> Self {
        Self { peer, key_store }
    }
}
