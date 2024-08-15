use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Puzzle, SpendContext};

/// An individual layer in a puzzle's hierarchy.
pub trait Layer {
    /// Most of the time, this is an actual CLVM type representing the solution.
    /// However, you can also use a helper struct and customize [`Layer::construct_solution`] and [`Layer::parse_solution`].
    type Solution;

    /// Parses this layer from the given puzzle, returning [`None`] if the puzzle doesn't match.
    /// An error is returned if the puzzle should have matched but couldn't be parsed.
    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError>
    where
        Self: Sized;

    /// Parses the [`Layer::Solution`] type from a CLVM solution pointer.
    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError>;

    /// Constructs the full curried puzzle for this layer.
    /// Ideally, the puzzle itself should be cached in the [`SpendContext`].
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError>;

    /// Constructs the full solution for this layer.
    /// Can be used to construct the solution from a helper struct, if it's not directly a CLVM type.
    /// It's also possible to influence the solution based on the puzzle, if needed.
    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError>;
}
