use std::{future::Future, ops::Range};

use crate::{CoinStore, KeyStore};

/// Keeps track of derived puzzle hashes in a wallet, based on its public keys.
pub trait PuzzleStore: KeyStore {
    /// Gets the derivation index of a puzzle hash.
    fn puzzle_hash_index(&self, puzzle_hash: [u8; 32]) -> impl Future<Output = Option<u32>> + Send;

    /// Gets the puzzle hash at a given index.
    fn puzzle_hash(&self, index: u32) -> impl Future<Output = Option<[u8; 32]>> + Send;

    /// Gets all of the puzzle hashes.
    fn puzzle_hashes(&self) -> impl Future<Output = Vec<[u8; 32]>> + Send;
}

pub async fn unused_indices(
    puzzle_store: &impl PuzzleStore,
    coin_store: &impl CoinStore,
) -> Result<Range<u32>, u32> {
    let mut index = None;
    let puzzle_hashes = puzzle_store.puzzle_hashes().await;
    let len = puzzle_hashes.len() as u32;

    for (i, puzzle_hash) in puzzle_hashes.into_iter().enumerate().rev() {
        if coin_store.is_used(puzzle_hash).await {
            break;
        }
        index = Some(i as u32);
    }

    match index {
        Some(index) => Ok(index..len),
        None => Err(len),
    }
}
