use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{DriverError, SpendContext};

use super::KnownPuzzles;

pub trait VaultLayer {
    #[must_use]
    fn replace(self, known_puzzles: &KnownPuzzles) -> Self;
    fn puzzle_hash(&self) -> TreeHash;
    fn puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError>;
}
