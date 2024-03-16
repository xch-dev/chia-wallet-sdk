use std::future::Future;

use crate::KeyStore;

/// Keeps track of derived puzzle hashes in a wallet, based on its public keys.
pub trait PuzzleStore: KeyStore {
    /// Gets the derivation index of a puzzle hash.
    fn puzzle_hash_index(&self, puzzle_hash: [u8; 32]) -> impl Future<Output = Option<u32>> + Send;

    /// Gets the puzzle hash at a given index.
    fn puzzle_hash(&self, index: u32) -> impl Future<Output = Option<[u8; 32]>> + Send;

    /// Gets all of the puzzle hashes.
    fn puzzle_hashes(&self) -> impl Future<Output = Vec<[u8; 32]>> + Send;
}
