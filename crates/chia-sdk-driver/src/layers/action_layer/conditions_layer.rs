use chia_wallet_sdk::{
    driver::{DriverError, Layer, Puzzle, Spend, SpendContext},
    types::{Condition, Conditions},
};
use clvm_traits::{clvm_quote, match_quote, FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

/// The Conditions [`Layer`] is a puzzle that simply returns the conditions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionsLayer<T = NodePtr> {
    pub conditions: Conditions<T>,
}

impl<T> ConditionsLayer<T> {
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
    pub fn spend(self, ctx: &mut SpendContext) -> Result<Spend, DriverError> {
        let puzzle = self.construct_puzzle(ctx)?;
        let solution = self.construct_solution(ctx, ())?;

        Ok(Spend { puzzle, solution })
    }
}
