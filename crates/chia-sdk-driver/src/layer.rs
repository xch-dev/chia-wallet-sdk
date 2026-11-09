use chia_protocol::{Coin, CoinSpend};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Puzzle, Spend, SpendContext};

/// An individual layer in a puzzle's hierarchy, which can be constructed and parsed.
///
/// Most puzzles are made of layers that wrap an inner puzzle, such as a [`CatLayer`](crate::CatLayer)
/// around a [`StandardLayer`](crate::StandardLayer). Layers that wrap an inner puzzle are generic
/// over it, so they compose freely, and constructing or parsing the outer layer also constructs
/// or parses the inner layers. The same goes for solutions.
///
/// Any type that implements both [`ToClvm`] and [`FromClvm`] (such as [`NodePtr`]) is also a layer,
/// which allocates the value itself as the puzzle and takes a [`NodePtr`] solution. This is useful
/// as an inner puzzle when you don't know or care what it is. [`Puzzle`] is also a layer, which
/// keeps the puzzle that was parsed.
///
/// # Example
///
/// Constructing a CAT puzzle, and parsing it back into its layers:
///
/// ```
/// use chia_sdk_driver::{CatLayer, Layer, Puzzle, SpendContext, StandardLayer};
/// use chia_sdk_test::BlsPair;
/// use chia_protocol::Bytes32;
/// use clvm_utils::ToTreeHash;
///
/// let mut ctx = SpendContext::new();
/// let alice = BlsPair::new(0);
///
/// let layer = CatLayer::new(Bytes32::new([1; 32]), StandardLayer::new(alice.pk));
/// let ptr = layer.construct_puzzle(&mut ctx)?;
///
/// // The puzzle hash can also be calculated without allocating the puzzle
/// assert_eq!(ctx.tree_hash(ptr), layer.tree_hash());
///
/// let puzzle = Puzzle::parse(&ctx, ptr);
/// let parsed = CatLayer::<StandardLayer>::parse_puzzle(&ctx, puzzle)?;
/// assert_eq!(parsed, Some(layer));
///
/// // A puzzle that doesn't match the layer isn't an error
/// let ptr = layer.inner_puzzle.construct_puzzle(&mut ctx)?;
/// let puzzle = Puzzle::parse(&ctx, ptr);
/// assert_eq!(CatLayer::<StandardLayer>::parse_puzzle(&ctx, puzzle)?, None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub trait Layer {
    /// The solution of this layer, including the solutions of any inner layers.
    ///
    /// Most of the time, this is a CLVM type representing the solution. However, it can also be a
    /// helper type, in which case [`Layer::construct_solution`] and [`Layer::parse_solution`]
    /// convert it to and from CLVM.
    type Solution;

    /// Parses this layer from the given puzzle, returning [`None`] if the puzzle doesn't match.
    ///
    /// An error is returned if the puzzle should have matched but couldn't be parsed, for example
    /// if its mod hash matches but its curried arguments are malformed.
    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError>
    where
        Self: Sized;

    /// Parses the [`Layer::Solution`] type from a CLVM solution pointer.
    ///
    /// This doesn't depend on the puzzle, so the inner layers are parsed using only their types.
    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError>;

    /// Constructs the full curried puzzle for this layer, including any inner layers.
    ///
    /// The uncurried mod should be cached in the [`SpendContext`], for example with
    /// [`SpendContext::curry`], so that it's only deserialized once.
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError>;

    /// Constructs the full solution for this layer, including the solutions of any inner layers.
    ///
    /// This can construct the solution from a helper type, if [`Layer::Solution`] isn't directly a
    /// CLVM type. The solution can also depend on the layer's own values, if needed.
    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError>;

    /// Constructs the puzzle and solution of this layer as a [`Spend`].
    fn construct_spend(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<Spend, DriverError> {
        let solution = self.construct_solution(ctx, solution)?;
        let puzzle = self.construct_puzzle(ctx)?;
        Ok(Spend::new(puzzle, solution))
    }

    /// Constructs the puzzle and solution of this layer, and serializes them into a [`CoinSpend`]
    /// for the given coin.
    ///
    /// The coin spend is returned rather than added to the [`SpendContext`], so use
    /// [`SpendContext::insert`] to include it in the transaction.
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
