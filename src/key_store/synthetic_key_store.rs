use chia_bls::{
    derive_keys::master_to_wallet_unhardened_intermediate, sign, DerivableKey, PublicKey,
    SecretKey, Signature,
};
use chia_wallet::{standard::DEFAULT_HIDDEN_PUZZLE_HASH, DeriveSynthetic};
use indexmap::IndexMap;

use crate::{KeyStore, Signer};

pub struct SyntheticKeyStore {
    intermediate_key: SecretKey,
    hidden_puzzle_hash: [u8; 32],
    keys: IndexMap<PublicKey, SecretKey>,
}

impl SyntheticKeyStore {
    pub fn new(root_key: &SecretKey) -> Self {
        Self {
            intermediate_key: master_to_wallet_unhardened_intermediate(root_key),
            hidden_puzzle_hash: DEFAULT_HIDDEN_PUZZLE_HASH,
            keys: IndexMap::new(),
        }
    }

    pub fn with_hidden_puzzle_hash(mut self, hidden_puzzle_hash: [u8; 32]) -> Self {
        self.hidden_puzzle_hash = hidden_puzzle_hash;
        self
    }
}

impl KeyStore for SyntheticKeyStore {
    async fn public_key(&self, index: u32) -> PublicKey {
        self.keys.get_index(index as usize).unwrap().0.clone()
    }

    async fn public_keys(&self) -> Vec<PublicKey> {
        self.keys.iter().map(|key| key.0).cloned().collect()
    }

    async fn derive_to_index(&mut self, index: u32) {
        let current = self.keys.len() as u32;
        for index in current..index {
            let secret_key = self
                .intermediate_key
                .derive_unhardened(index)
                .derive_synthetic(&self.hidden_puzzle_hash);
            let public_key = secret_key.public_key();
            self.keys.insert(public_key, secret_key);
        }
    }
}

impl Signer for SyntheticKeyStore {
    async fn secret_key(&self, index: u32) -> SecretKey {
        self.keys.get_index(index as usize).unwrap().1.clone()
    }

    async fn sign_message(&self, public_key: &PublicKey, message: &[u8]) -> Signature {
        sign(&self.keys[public_key], message)
    }
}
