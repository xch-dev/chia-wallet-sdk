use clvmr::NodePtr;

/// A puzzle and solution pair, allocated in a [`SpendContext`](crate::SpendContext).
///
/// Unlike a [`CoinSpend`](chia_protocol::CoinSpend), a [`Spend`] isn't tied to a coin and isn't
/// serialized. This makes it the common currency between layers: an inner puzzle's spend can be
/// wrapped by an outer layer, or passed as a delegated spend. Use
/// [`SpendContext::spend`](crate::SpendContext::spend) to serialize it for a coin.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct Spend {
    /// The puzzle being run.
    pub puzzle: NodePtr,
    /// The solution passed to the puzzle.
    pub solution: NodePtr,
}

impl Spend {
    /// Creates a [`Spend`] from a puzzle and solution.
    pub fn new(puzzle: NodePtr, solution: NodePtr) -> Self {
        Self { puzzle, solution }
    }
}
