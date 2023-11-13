use chia_bls::PublicKey;
use chia_wallet::cat::CAT_PUZZLE_HASH;
use clvm_utils::{curry_tree_hash, tree_hash_atom};

use crate::PuzzleGenerator;

#[derive(Debug, Clone, Copy)]
pub struct CatPuzzleGenerator<I>
where
    I: PuzzleGenerator,
{
    asset_id: [u8; 32],
    inner_puzzle_generator: I,
}

impl<I> CatPuzzleGenerator<I>
where
    I: PuzzleGenerator,
{
    pub fn new(asset_id: [u8; 32], inner_puzzle_generator: I) -> Self {
        Self {
            asset_id,
            inner_puzzle_generator,
        }
    }
}

impl<I> PuzzleGenerator for CatPuzzleGenerator<I>
where
    I: PuzzleGenerator,
{
    fn puzzle_hash(&self, public_key: &PublicKey) -> [u8; 32] {
        cat_puzzle_hash(
            self.asset_id,
            self.inner_puzzle_generator.puzzle_hash(public_key),
        )
    }
}

pub fn cat_puzzle_hash(asset_id: [u8; 32], inner_puzzle_hash: [u8; 32]) -> [u8; 32] {
    let mod_hash = tree_hash_atom(&CAT_PUZZLE_HASH);
    let asset_id_hash = tree_hash_atom(&asset_id);
    curry_tree_hash(
        CAT_PUZZLE_HASH,
        &[mod_hash, asset_id_hash, inner_puzzle_hash],
    )
}
