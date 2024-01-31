use std::future::Future;

use chia_bls::PublicKey;

/// Keeps track of and derives wallet public keys by index.
pub trait PublicKeyStore {
    /// Gets the number of public keys.
    fn count(&self) -> impl Future<Output = u32> + Send;

    /// Gets the public key at a given index.
    fn public_key(&self, index: u32) -> impl Future<Output = Option<PublicKey>> + Send;

    /// Gets the derivation index of a public key.
    fn index_of_pk(&self, public_key: &PublicKey) -> impl Future<Output = Option<u32>> + Send;

    /// Generates a keypair and puzzle hash for each derivation up to the index.
    fn derive_to_index(&self, index: u32) -> impl Future<Output = ()> + Send;
}
