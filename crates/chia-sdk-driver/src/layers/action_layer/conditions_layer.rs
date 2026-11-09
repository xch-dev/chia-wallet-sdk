use chia_sdk_types::{Condition, Conditions};
use clvm_traits::{FromClvm, ToClvm, clvm_quote, match_quote};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

/// The conditions [`Layer`] is a quoted list of conditions, a puzzle that ignores its solution
/// and always outputs the same [`Conditions`].
///
/// It's useful as a delegated puzzle, for example in [`MOfNLayer::spend_with_conditions`](crate::MOfNLayer::spend_with_conditions).
/// The solution is `()`, and [`Layer::parse_puzzle`] returns an error if the puzzle is a
/// quoted value that isn't a list of conditions.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_driver::{ConditionsLayer, Layer, Puzzle, SpendContext};
/// use chia_sdk_types::{Condition, Conditions};
///
/// let ctx = &mut SpendContext::new();
///
/// let conditions = Conditions::new().reserve_fee(1);
/// let spend = ConditionsLayer::new(conditions.clone()).spend(ctx)?;
///
/// let parsed = ConditionsLayer::parse_puzzle(ctx, Puzzle::parse(ctx, spend.puzzle))?;
/// assert_eq!(parsed, Some(ConditionsLayer::new(conditions)));
///
/// let output = ctx.run(spend.puzzle, spend.solution)?;
/// let output = ctx.extract::<Vec<Condition>>(output)?;
/// assert_eq!(output, vec![Condition::reserve_fee(1)]);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionsLayer<T = NodePtr> {
    /// The conditions output by the puzzle.
    pub conditions: Conditions<T>,
}

impl<T> ConditionsLayer<T> {
    /// Creates a new [`ConditionsLayer`] that outputs `conditions`.
    pub fn new(conditions: Conditions<T>) -> Self {
        Self { conditions }
    }
}

impl<T> Layer for ConditionsLayer<T>
where
    T: FromClvm<Allocator> + ToClvm<Allocator> + Clone,
{
    type Solution = ();

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_raw() else {
            return Ok(None);
        };

        let (_q, conditions) = <match_quote!(Vec<Condition<T>>)>::from_clvm(allocator, puzzle.ptr)?;

        Ok(Some(Self::new(
            Conditions::<T>::default().extend(conditions),
        )))
    }

    fn parse_solution(_: &Allocator, _: NodePtr) -> Result<Self::Solution, DriverError> {
        Ok(())
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        Ok(clvm_quote!(self.conditions.clone()).to_clvm(ctx)?)
    }

    fn construct_solution(
        &self,
        _: &mut SpendContext,
        (): Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        Ok(NodePtr::NIL)
    }
}

impl<T> ConditionsLayer<T>
where
    T: FromClvm<Allocator> + ToClvm<Allocator> + Clone,
{
    /// Constructs the puzzle and solution of this layer as a [`Spend`].
    pub fn spend(self, ctx: &mut SpendContext) -> Result<Spend, DriverError> {
        let puzzle = self.construct_puzzle(ctx)?;
        let solution = self.construct_solution(ctx, ())?;

        Ok(Spend { puzzle, solution })
    }
}
