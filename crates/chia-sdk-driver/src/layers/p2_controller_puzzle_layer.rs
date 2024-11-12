use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, TreeHash};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

pub const P2_CONTROLLER_PUZZLE_PUZZLE: [u8; 151] = hex!("ff02ffff01ff04ffff04ff04ffff04ffff0117ffff04ffff02ff06ffff04ff02ffff04ff0bff80808080ffff04ff05ff8080808080ffff02ff0bff178080ffff04ffff01ff43ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080ffff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080");
pub const P2_CONTROLLER_PUZZLE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    d5415713619e318bfa7820e06e2b163beef32d82294a5a7fcf9c3c69b0949c88
    "
));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2ControllerPuzzleLayer {
    pub controller_puzzle_hash: Bytes32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2ControllerPuzzleArgs {
    pub controller_puzzle_hash: Bytes32,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct P2ControllerPuzzleSolution<P, S> {
    pub delegated_puzzle: P,
    pub delegated_solution: S,
}

impl P2ControllerPuzzleLayer {
    pub fn new(controller_puzzle_hash: Bytes32) -> Self {
        Self {
            controller_puzzle_hash,
        }
    }

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

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.p2_controller_puzzle_puzzle()?,
            args: P2ControllerPuzzleArgs {
                controller_puzzle_hash: self.controller_puzzle_hash,
            },
        };
        ctx.alloc(&curried)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }

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
        Ok(Self::Solution::from_clvm(allocator, solution)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::assert_puzzle_hash;

    use super::*;
    use chia_protocol::{Bytes, CoinSpend};
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;
    use clvm_traits::clvm_quote;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(P2_CONTROLLER_PUZZLE_PUZZLE => P2_CONTROLLER_PUZZLE_PUZZLE_HASH);

        Ok(())
    }

    #[test]
    fn test_p2_controller_puzzle() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let controller_puzzle = ctx.allocator.one();
        let controller_puzzle_hash = ctx.tree_hash(controller_puzzle);

        let layer = P2ControllerPuzzleLayer::new(controller_puzzle_hash.into());
        let coin_puzzle = layer.construct_puzzle(ctx)?;
        let coin_puzzle_hash = ctx.tree_hash(coin_puzzle);

        let controller_coin = sim.new_coin(controller_puzzle_hash.into(), 42);
        let coin = sim.new_coin(coin_puzzle_hash.into(), 69);

        let delegated_puzzle =
            clvm_quote!(Conditions::new().reserve_fee(42 + 69)).to_clvm(&mut ctx.allocator)?;
        let delegated_solution = ctx.allocator.nil();

        let delegated_puzzle_hash = ctx.tree_hash(delegated_puzzle);

        let coin_spend = layer.construct_coin_spend(
            ctx,
            coin,
            P2ControllerPuzzleSolution {
                delegated_puzzle,
                delegated_solution,
            },
        )?;
        ctx.insert(coin_spend);

        let controller_solution = Conditions::new().send_message(
            23,
            Bytes::from(delegated_puzzle_hash.to_vec()),
            vec![coin.coin_id().to_clvm(&mut ctx.allocator)?],
        );
        let controller_solution = controller_solution.to_clvm(&mut ctx.allocator)?;

        let controller_coin_spend = CoinSpend::new(
            controller_coin,
            ctx.serialize(&controller_puzzle)?,
            ctx.serialize(&controller_solution)?,
        );
        ctx.insert(controller_coin_spend);

        sim.spend_coins(ctx.take(), &[])?;

        Ok(())
    }
}
