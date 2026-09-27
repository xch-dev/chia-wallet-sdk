use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::{
    P2_CONTROLLER_PUZZLE_PUZZLE_HASH, P2ControllerPuzzleArgs, P2ControllerPuzzleSolution,
};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

/// The CHIP-0037 controller-puzzle [`Layer`].
///
/// Locks a coin so it can only be spent when a coin whose puzzle hash equals
/// `controller_puzzle_hash` sends a `RECEIVE_MESSAGE` whose body is the tree
/// hash of the chosen `delegated_puzzle`. Useful for collapsing many
/// per-coin signatures into a single signature on the controller (for example
/// a [`P2Eip712MessageLayer`](super::P2Eip712MessageLayer) coin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2ControllerPuzzleLayer {
    pub controller_puzzle_hash: Bytes32,
}

impl P2ControllerPuzzleLayer {
    pub fn new(controller_puzzle_hash: Bytes32) -> Self {
        Self {
            controller_puzzle_hash,
        }
    }

    /// Spend a coin locked by this layer with a `delegated_spend` previously
    /// authorised by the controller coin.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        delegated_spend: Spend,
    ) -> Result<Spend, DriverError> {
        self.construct_spend(
            ctx,
            P2ControllerPuzzleSolution {
                delegated_puzzle: delegated_spend.puzzle,
                delegated_solution: delegated_spend.solution,
            },
        )
    }
}

impl Layer for P2ControllerPuzzleLayer {
    type Solution = P2ControllerPuzzleSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_CONTROLLER_PUZZLE_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2ControllerPuzzleArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            controller_puzzle_hash: args.controller_puzzle_hash,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2ControllerPuzzleSolution::from_clvm(allocator, solution)?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2ControllerPuzzleArgs {
            controller_puzzle_hash: self.controller_puzzle_hash,
        })
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chia_protocol::Bytes;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::{Conditions, conditions::SendMessage};
    use clvm_traits::{ToClvm, clvm_quote};

    /// A controller coin spend authorising a delegated puzzle for one
    /// controlled coin succeeds, demonstrating the one-message-many-spends
    /// workflow described in CHIP-0037.
    #[test]
    fn test_p2_controller_puzzle() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let ctx = &mut ctx;

        // The controller is a degenerate quoted-conditions coin so that we can
        // emit any conditions we want to test the message-routing semantics.
        let controller_puzzle = ctx.one();
        let controller_puzzle_hash: Bytes32 = ctx.tree_hash(controller_puzzle).into();

        let layer = P2ControllerPuzzleLayer::new(controller_puzzle_hash);
        let coin_puzzle = layer.construct_puzzle(ctx)?;
        let coin_puzzle_hash = ctx.tree_hash(coin_puzzle);

        let controller_coin = sim.new_coin(controller_puzzle_hash, 42);
        let coin = sim.new_coin(coin_puzzle_hash.into(), 69);

        let delegated_puzzle =
            clvm_quote!(Conditions::new().reserve_fee(42 + 69)).to_clvm(&mut **ctx)?;
        let delegated_solution = ctx.nil();

        let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_puzzle).into();

        let coin_spend = layer.construct_coin_spend(
            ctx,
            coin,
            P2ControllerPuzzleSolution {
                delegated_puzzle,
                delegated_solution,
            },
        )?;
        ctx.insert(coin_spend);

        // mode 0b010111 == puzzle hash → coin
        let coin_id_ptr = ctx.alloc(&coin.coin_id())?;
        let controller_solution = vec![SendMessage::new(
            23,
            Bytes::new(delegated_puzzle_hash.to_vec()),
            vec![coin_id_ptr],
        )]
        .to_clvm(&mut **ctx)?;

        let controller_spend = chia_protocol::CoinSpend::new(
            controller_coin,
            chia_protocol::Program::from_clvm(&**ctx, controller_puzzle)?,
            chia_protocol::Program::from_clvm(&**ctx, controller_solution)?,
        );
        ctx.insert(controller_spend);

        sim.spend_coins(ctx.take(), &[])?;
        Ok(())
    }
}
