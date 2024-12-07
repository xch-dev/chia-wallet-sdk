use chia_protocol::{Coin, CoinSpend};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Puzzle, Spend, SpendContext};

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

    /// Creates a spend for this layer.
    fn construct_spend(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<Spend, DriverError> {
        let solution = self.construct_solution(ctx, solution)?;
        let puzzle = self.construct_puzzle(ctx)?;
        Ok(Spend::new(puzzle, solution))
    }

    /// Creates a coin spend for this layer.
    fn construct_coin_spend(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        solution: Self::Solution,
    ) -> Result<CoinSpend, DriverError> {
        let solution = self.construct_solution(ctx, solution)?;
        let puzzle = self.construct_puzzle(ctx)?;
        Ok(CoinSpend::new(
            coin,
            ctx.serialize(&puzzle)?,
            ctx.serialize(&solution)?,
        ))
    }
}

impl<T> Layer for T
where
    T: ToClvm<Allocator> + FromClvm<Allocator>,
{
    type Solution = NodePtr;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        Ok(Some(T::from_clvm(allocator, puzzle.ptr())?))
    }

    fn parse_solution(
        _allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(solution)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.alloc(&self)
    }

    fn construct_solution(
        &self,
        _ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        Ok(solution)
    }
}
