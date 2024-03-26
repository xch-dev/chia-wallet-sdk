use std::future::Future;

use crate::PublicKeyStore;

mod pk_derivation_store;
mod sk_derivation_store;

use chia_protocol::Bytes32;
pub use pk_derivation_store::*;
pub use sk_derivation_store::*;

/// Keeps track of derived puzzle hashes in a wallet, based on its public keys.
pub trait DerivationStore: PublicKeyStore {
    /// Gets the derivation index of a puzzle hash.
    fn index_of_ph(&self, puzzle_hash: Bytes32) -> impl Future<Output = Option<u32>> + Send;

    /// Gets the puzzle hash at a given index.
    fn puzzle_hash(&self, index: u32) -> impl Future<Output = Option<Bytes32>> + Send;

    /// Gets all of the puzzle hashes.
    fn puzzle_hashes(&self) -> impl Future<Output = Vec<Bytes32>> + Send;
}
