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
    fn next_derivation_index(&self) -> u32 {
        self.keys.len() as u32
    }

    fn derive_keys(&mut self, count: u32) -> Vec<PublicKey> {
        let next = self.next_derivation_index();
        let mut public_keys = Vec::new();
        for index in next..(next + count) {
            let secret_key = self
                .intermediate_key
                .derive_unhardened(index)
                .derive_synthetic(&self.hidden_puzzle_hash);
            let public_key = secret_key.public_key();
            public_keys.push(public_key.clone());
            self.keys.insert(public_key, secret_key);
        }
        public_keys
    }

    fn public_key(&self, index: u32) -> PublicKey {
        self.keys.get_index(index as usize).unwrap().0.clone()
    }
}

impl Signer for SyntheticKeyStore {
    fn has_public_key(&self, public_key: &PublicKey) -> bool {
        self.keys.contains_key(public_key)
    }

    fn sign_message(&self, public_key: &PublicKey, message: &[u8]) -> Signature {
        sign(&self.keys[public_key], message)
    }
}
