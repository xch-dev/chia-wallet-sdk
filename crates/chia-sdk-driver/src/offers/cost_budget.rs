use chia_sdk_types::{MAINNET_CONSTANTS, run_puzzle_with_cost};
use clvmr::{Allocator, NodePtr, error::EvalErr};

use crate::DriverError;

/// The total cost that can be spent running the puzzles of an untrusted offer while parsing it.
///
/// Every puzzle in an offer is chosen by its maker, so without a total budget an offer with many
/// expensive spends could take a very long time to parse. Each spend is run in mempool mode
/// against the cost that remains, so the whole offer costs no more than a single block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostBudget {
    remaining: u64,
}

impl CostBudget {
    pub fn new(max_cost: u64) -> Self {
        Self {
            remaining: max_cost,
        }
    }

    /// The default budget for parsing an offer, which is the maximum cost of a block.
    pub fn offer() -> Self {
        Self::new(MAINNET_CONSTANTS.max_block_cost_clvm)
    }

    pub fn remaining(&self) -> u64 {
        self.remaining
    }

    /// Runs the puzzle in mempool mode, and deducts its cost from the budget. Returns
    /// [`DriverError::OfferCostExceeded`] if it costs more than the remaining budget.
    pub fn run(
        &mut self,
        allocator: &mut Allocator,
        puzzle: NodePtr,
        solution: NodePtr,
    ) -> Result<(u64, NodePtr), DriverError> {
        // A maximum cost of zero means there is no limit.
        if self.remaining == 0 {
            return Err(DriverError::OfferCostExceeded);
        }

        let reduction = run_puzzle_with_cost(allocator, puzzle, solution, self.remaining, true)
            .map_err(|error| match error {
                EvalErr::CostExceeded => DriverError::OfferCostExceeded,
                error => DriverError::Eval(error),
            })?;

        self.charge(reduction.0)?;

        Ok((reduction.0, reduction.1))
    }

    /// Deducts the cost from the budget. Returns [`DriverError::OfferCostExceeded`] if it's more
    /// than the remaining budget.
    pub fn charge(&mut self, cost: u64) -> Result<(), DriverError> {
        self.remaining = self
            .remaining
            .checked_sub(cost)
            .ok_or(DriverError::OfferCostExceeded)?;

        Ok(())
    }
}

impl Default for CostBudget {
    fn default() -> Self {
        Self::offer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_budget() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let puzzle = allocator.new_small_number(1)?;
        let solution = allocator.new_small_number(42)?;

        let mut budget = CostBudget::new(1_000);
        let (cost, output) = budget.run(&mut allocator, puzzle, solution)?;
        assert_eq!(output, solution);
        assert_eq!(budget.remaining(), 1_000 - cost);

        // The budget is shared across runs, so it eventually runs out
        let mut budget = CostBudget::new(cost);
        budget.run(&mut allocator, puzzle, solution)?;
        assert!(matches!(
            budget.run(&mut allocator, puzzle, solution),
            Err(DriverError::OfferCostExceeded)
        ));

        Ok(())
    }
}
