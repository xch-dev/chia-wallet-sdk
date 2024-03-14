use std::future::Future;

mod simple_derivation_store;

use chia_bls::PublicKey;
use chia_wallet::standard::standard_puzzle_hash;
pub use simple_derivation_store::*;

use crate::KeyStore;

/// Keeps track of derived puzzle hashes in a wallet, based on its public keys.
pub trait DerivationStore: KeyStore {
    /// Gets the derivation index of a puzzle hash.
    fn puzzle_hash_index(&self, puzzle_hash: [u8; 32]) -> impl Future<Output = Option<u32>> + Send;

    /// Gets the puzzle hash at a given index.
    fn puzzle_hash(&self, index: u32) -> impl Future<Output = Option<[u8; 32]>> + Send;

    /// Gets all of the puzzle hashes.
    fn puzzle_hashes(&self) -> impl Future<Output = Vec<[u8; 32]>> + Send;
}

/// Used to derive puzzle hashes from synthetic public keys.
pub trait PuzzleGenerator {
    /// Derives a puzzle hash from a given synthetic public key.
    fn puzzle_hash(&self, synthetic_pk: &PublicKey) -> [u8; 32];
}

pub struct StandardPuzzleGenerator;

impl PuzzleGenerator for StandardPuzzleGenerator {
    fn puzzle_hash(synthetic_pk: &PublicKey) -> [u8; 32] {
        standard_puzzle_hash(synthetic_pk)
    }
}

pub struct CatPuzzleGenerator;

impl PuzzleGenerator for CatPuzzleGenerator {
    fn puzzle_hash(&self, synthetic_pk: &PublicKey) -> [u8; 32] {}
}
