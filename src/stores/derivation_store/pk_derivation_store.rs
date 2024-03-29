use chia_bls::{derive_keys::master_to_wallet_unhardened_intermediate, DerivableKey, PublicKey};
use chia_protocol::Bytes32;
use chia_wallet::{
    standard::{standard_puzzle_hash, DEFAULT_HIDDEN_PUZZLE_HASH},
    DeriveSynthetic,
};
use indexmap::IndexMap;
use parking_lot::Mutex;

use crate::{DerivationStore, PublicKeyStore};

/// An in-memory derivation store implementation.
pub struct PkDerivationStore {
    intermediate_pk: PublicKey,
    hidden_puzzle_hash: Bytes32,
    derivations: Mutex<IndexMap<PublicKey, Bytes32>>,
}

impl PkDerivationStore {
    /// Creates a new key store with the default hidden puzzle hash.
    /// An intermediate secret key is derived from the root key.
    pub fn new(root_key: &PublicKey) -> Self {
        Self {
            intermediate_pk: master_to_wallet_unhardened_intermediate(root_key),
            hidden_puzzle_hash: DEFAULT_HIDDEN_PUZZLE_HASH.into(),
            derivations: Mutex::new(IndexMap::new()),
        }
    }

    /// Creates a new key store with a custom hidden puzzle hash.
    /// An intermediate secret key is derived from the root key.
    pub fn new_with_hidden_puzzle(root_key: &PublicKey, hidden_puzzle_hash: Bytes32) -> Self {
        let mut key_store = Self::new(root_key);
        key_store.hidden_puzzle_hash = hidden_puzzle_hash;
        key_store
    }
}

impl DerivationStore for PkDerivationStore {
    async fn index_of_ph(&self, puzzle_hash: Bytes32) -> Option<u32> {
        self.derivations
            .lock()
            .iter()
            .position(|derivation| *derivation.1 == puzzle_hash)
            .map(|index| index as u32)
    }

    async fn puzzle_hash(&self, index: u32) -> Option<Bytes32> {
        self.derivations
            .lock()
            .get_index(index as usize)
            .map(|derivation| *derivation.1)
    }

    async fn puzzle_hashes(&self) -> Vec<Bytes32> {
        self.derivations.lock().values().copied().collect()
    }
}

impl PublicKeyStore for PkDerivationStore {
    async fn count(&self) -> u32 {
        self.derivations.lock().len() as u32
    }

    async fn public_key(&self, index: u32) -> Option<PublicKey> {
        self.derivations
            .lock()
            .get_index(index as usize)
            .map(|derivation| derivation.0.clone())
    }

    async fn index_of_pk(&self, public_key: &PublicKey) -> Option<u32> {
        self.derivations
            .lock()
            .get_index_of(public_key)
            .map(|index| index as u32)
    }

    async fn derive_to_index(&self, index: u32) {
        let mut derivations = self.derivations.lock();
        let current = derivations.len() as u32;
        for index in current..index {
            let public_key = self
                .intermediate_pk
                .derive_unhardened(index)
                .derive_synthetic(&self.hidden_puzzle_hash.to_bytes());
            let puzzle_hash = standard_puzzle_hash(&public_key);
            derivations.insert(public_key, puzzle_hash.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use hex::ToHex;

    use crate::testing::SECRET_KEY;

    use super::*;

    #[tokio::test]
    async fn test_key_pairs() {
        let root_pk = SECRET_KEY.public_key();
        let store = PkDerivationStore::new(&root_pk);

        // Derive the first 10 keys.
        store.derive_to_index(10).await;

        let pks: Vec<PublicKey> = store.derivations.lock().keys().cloned().collect();
        let pks_hex: Vec<String> = pks.iter().map(|pk| pk.to_bytes().encode_hex()).collect();

        let expected_pks_hex = vec![
            "8584adae5630842a1766bc444d2b872dd3080f4e5daaecf6f762a4be7dc148f37868149d4217f3dcc9183fe61e48d8bf",
            "b07c0a00a30501d18418df3ece3335d2c7339e0589e61b9230cffc9573d0df739726e84e55e91d68744b0f3791285b96",
            "963eea603ce281d63daca66f0926421f51d6d24027e498cb9d02f6477e3e01c4c4fda666fc3ea4199fdf566244ba74e0",
            "b33bbccea1926947b7a83080c8b6a193121bf3480411abeb5fb31fa70002c150ba1d40a5c6a53b36cdd51ea468f0c2e4",
            "a7bf25f67541a4e292a06282d714bbbc203a8bd6b0d0b804d097a071388f84665659a1a1f220130d97bcd2c4775f1077",
            "a8fa6e4e7732e36d6e4e537c172a2c1e7fd926a43abd191c5aa82974a54e9de1addb32ea404724722dedc78407bbb098",
            "b40b3c77251cea8e4c9cbbecbaa7fe40e9ad5e1298c83696d879cffd0c28f9ed61d5f3aec34eb44593861b8d8aba796e",
            "94e949fd1ea33ac4886511c39ee3b98d2580a6fd66d2bb8517de0a1cd0afefea29702b1f6a3e88e74ce0686c7d53bde8",
            "b042fccde247d98b363c6edb1d921da2b099493e00713ba8d44b3d777901f33b41dd496f58baff1c1fc725e3f16f4b13",
            "a67d7a1f2c0754f97f9db696fb95c9f5462eb0a3fcb60dc072aebfad1ff3faabb6dd8f769f37c2e4df01af81863e410c",
        ];
        assert_eq!(pks_hex, expected_pks_hex);
    }
}
