use clvmr::NodePtr;

use crate::{Cat, Puzzle};

/// A [`Cat`] parsed from a coin spend by [`Cat::parse`], along with its revealed inner spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedCat {
    /// The CAT that was spent.
    pub cat: Cat,
    /// The puzzle revealed inside the CAT layer. If [`revoked`](Self::revoked) is set,
    /// this is the hidden puzzle rather than the p2 puzzle.
    pub p2_puzzle: Puzzle,
    /// The solution to [`p2_puzzle`](Self::p2_puzzle).
    pub p2_solution: NodePtr,
    /// Whether the hidden puzzle of the revocation layer was spent instead of the p2 puzzle.
    pub revoked: bool,
}
