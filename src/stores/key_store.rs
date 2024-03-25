use std::future::Future;

use chia_bls::PublicKey;

/// Keeps track of and derives wallet public keys by index.
pub trait KeyStore {
    /// Gets the number of public keys.
    fn count(&self) -> impl Future<Output = u32> + Send;

    /// Gets the public key at a given index.
    fn public_key(&self, index: u32) -> impl Future<Output = Option<PublicKey>> + Send;

    /// Gets the derivation index of a public key.
    fn public_key_index(&self, public_key: &PublicKey) -> impl Future<Output = Option<u32>> + Send;
}
