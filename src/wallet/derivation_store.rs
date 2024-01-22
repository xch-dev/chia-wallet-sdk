use std::future::Future;

use chia_bls::{
    derive_keys::master_to_wallet_unhardened_intermediate, DerivableKey, PublicKey, SecretKey,
};
use chia_wallet::{
    standard::{standard_puzzle_hash, DEFAULT_HIDDEN_PUZZLE_HASH},
    DeriveSynthetic,
};
use indexmap::IndexMap;
use parking_lot::Mutex;

use crate::Signer;

/// Responsible for managing wallet derivations.
pub trait DerivationStore {
    /// Gets the number of derivations.
    fn derivations(&self) -> impl Future<Output = u32> + Send;

    /// Gets the derivation index of a puzzle hash.
    fn index_of_puzzle_hash(
        &self,
        puzzle_hash: [u8; 32],
    ) -> impl Future<Output = Option<u32>> + Send;

    /// Gets the public key at a given index.
    fn public_key(&self, index: u32) -> impl Future<Output = Option<PublicKey>> + Send;

    /// Gets the puzzle hash at a given index.
    fn puzzle_hash(&self, index: u32) -> impl Future<Output = Option<[u8; 32]>> + Send;

    /// Generates a keypair and puzzle hash for each derivation up to the index.
    fn derive_to_index(&self, index: u32) -> impl Future<Output = ()> + Send;
}

/// An in-memory derivation store implementation.
pub struct MemorySkDerivationStore {
    intermediate_sk: SecretKey,
    hidden_puzzle_hash: [u8; 32],
    derivations: Mutex<IndexMap<PublicKey, SkDerivation>>,
}

#[derive(Clone)]
struct SkDerivation {
    secret_key: SecretKey,
    puzzle_hash: [u8; 32],
}

impl MemorySkDerivationStore {
    /// Creates a new key store with the default hidden puzzle hash.
    /// An intermediate secret key is derived from the root key.
    pub fn new(root_key: &SecretKey) -> Self {
        Self {
            intermediate_sk: master_to_wallet_unhardened_intermediate(root_key),
            hidden_puzzle_hash: DEFAULT_HIDDEN_PUZZLE_HASH,
            derivations: Mutex::new(IndexMap::new()),
        }
    }

    /// Creates a new key store with a custom hidden puzzle hash.
    /// An intermediate secret key is derived from the root key.
    pub fn new_with_hidden_puzzle(root_key: &SecretKey, hidden_puzzle_hash: [u8; 32]) -> Self {
        let mut key_store = Self::new(root_key);
        key_store.hidden_puzzle_hash = hidden_puzzle_hash;
        key_store
    }
}

impl DerivationStore for MemorySkDerivationStore {
    async fn derivations(&self) -> u32 {
        self.derivations.lock().len() as u32
    }

    async fn index_of_puzzle_hash(&self, puzzle_hash: [u8; 32]) -> Option<u32> {
        self.derivations
            .lock()
            .iter()
            .position(|derivation| derivation.1.puzzle_hash == puzzle_hash)
            .map(|index| index as u32)
    }

    async fn public_key(&self, index: u32) -> Option<PublicKey> {
        self.derivations
            .lock()
            .get_index(index as usize)
            .map(|derivation| derivation.0.clone())
    }

    async fn puzzle_hash(&self, index: u32) -> Option<[u8; 32]> {
        self.derivations
            .lock()
            .get_index(index as usize)
            .map(|derivation| derivation.1.puzzle_hash)
    }

    async fn derive_to_index(&self, index: u32) {
        let mut derivations = self.derivations.lock();
        let current = derivations.len() as u32;
        for index in current..index {
            let secret_key = self
                .intermediate_sk
                .derive_unhardened(index)
                .derive_synthetic(&self.hidden_puzzle_hash);
            let public_key = secret_key.public_key();
            let puzzle_hash = standard_puzzle_hash(&public_key);
            derivations.insert(
                public_key,
                SkDerivation {
                    secret_key,
                    puzzle_hash,
                },
            );
        }
    }
}

impl Signer for MemorySkDerivationStore {
    async fn secret_key(&self, public_key: &PublicKey) -> Option<SecretKey> {
        self.derivations
            .lock()
            .get(public_key)
            .map(|derivation| derivation.secret_key.clone())
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::sign;
    use hex::ToHex;
    use hex_literal::hex;

    use crate::testing::SEED;

    use super::*;

    #[tokio::test]
    async fn test_key_pairs() {
        let root_sk = SecretKey::from_seed(SEED.as_ref());
        let store = MemorySkDerivationStore::new(&root_sk);

        // Derive the first 10 keys.
        store.derive_to_index(10).await;

        let sks: Vec<SecretKey> = store
            .derivations
            .lock()
            .values()
            .map(|derivation| derivation.secret_key.clone())
            .collect();
        let pks: Vec<PublicKey> = store.derivations.lock().keys().cloned().collect();

        let sks_hex: Vec<String> = sks.iter().map(|sk| sk.to_bytes().encode_hex()).collect();
        let pks_hex: Vec<String> = pks.iter().map(|pk| pk.to_bytes().encode_hex()).collect();

        let manual_pks_hex: Vec<String> = sks
            .iter()
            .map(|sk| sk.public_key().to_bytes().encode_hex())
            .collect();

        assert_eq!(&pks_hex, &manual_pks_hex);

        let expected_sks_hex = vec![
            "125e0b72383dfc25e125613331f1d0b3d011e4e66e06e851cdfbbcf4d32dfb46",
            "3d6e7e99226e190bc495938ac5be8d8689445bfaa6a6396f021473c2adc8ef7d",
            "36f3ac4d23877d2e90086864d28b9e0aa88e5fdc2ac08115bf11bacd8c1c4ccc",
            "01711de0ebf04952c7bc53ba9ff4463f063c60d4c507dd8ed4a6448d9d2ced08",
            "501aaeb127c2480976badc5601165b84d7906ed9c8754e05b2f65d5a6fdbc20b",
            "5093bccdb9936b10c6f330b10abbf7c2937e7ccc76a35704a2f1cee96c23e173",
            "013ec1bc4a37bea42cbc792ad23102f0759d2f941627b70dff039571e062301c",
            "1b9abaeb853ef0102ce5bb07f804e628fd846e2b7e67036b109dfd4b06414e81",
            "50394a6e095bd279b1ff1a095a15ecd561a66a5c3c5b9ab51214f97a3e68017e",
            "41a5f2ebd31a3e338aa7af91fb1235dbb02b053fbf38073e0de9b448b2d1fdb0",
        ];
        assert_eq!(sks_hex, expected_sks_hex);

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

    #[tokio::test]
    async fn test_sign_message() {
        let root_sk = SecretKey::from_seed(SEED.as_ref());
        let store = MemorySkDerivationStore::new(&root_sk);

        // Derive the first key.
        store.derive_to_index(1).await;

        let message = b"Hello, Chia blockchain!";

        let pk = store.public_key(0).await.unwrap();
        let sk = store.secret_key(&pk).await.unwrap();

        let sk_hex: String = sk.to_bytes().encode_hex();
        let pk_hex: String = pk.to_bytes().encode_hex();
        let manual_pk_hex: String = sk.public_key().to_bytes().encode_hex();

        assert_eq!(pk_hex, manual_pk_hex);
        assert_eq!(
            sk_hex,
            "125e0b72383dfc25e125613331f1d0b3d011e4e66e06e851cdfbbcf4d32dfb46"
        );
        assert_eq!(
            pk_hex,
            "8584adae5630842a1766bc444d2b872dd3080f4e5daaecf6f762a4be7dc148f37868149d4217f3dcc9183fe61e48d8bf"
        );

        let sig_hex: String = sign(&sk, message).to_bytes().encode_hex();

        assert_eq!(
            sig_hex,
            hex::encode(hex!(
                "
                a8cdf5167335be076807e285ed64e6ec649f560ee9f361265d918395fda3d583
                76fbe22967cea973a61495a50755716c1951d7f3429faebea09b968c8347fe7d
                1effa1285d944ed26d17481b01689c2c4c9c7ab2435388267a40f7355ed79dc2
                "
            ))
        );
    }
}
