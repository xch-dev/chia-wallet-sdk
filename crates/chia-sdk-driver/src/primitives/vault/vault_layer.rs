use clvm_utils::TreeHash;

use super::KnownPuzzles;

pub trait VaultLayer {
    #[must_use]
    fn replace(self, known_puzzles: &KnownPuzzles) -> Self;
    fn puzzle_hash(&self) -> TreeHash;
}
