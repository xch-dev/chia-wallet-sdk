use chia_protocol::{Bytes32, Coin};
use chia_puzzles::singleton::{SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH};
use chia_sdk_types::{
    P2DelegatedSingletonArgs, P2DelegatedSingletonSolution, P2_DELEGATED_SINGLETON_PUZZLE_HASH,
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

/// The p2 delegated singleton [`Layer`] allows for requiring that a singleton
/// be spent alongside this coin to authorize it, while also outputting conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2DelegatedSingletonLayer {
    pub launcher_id: Bytes32,
}

impl P2DelegatedSingletonLayer {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self { launcher_id }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        coin_id: Bytes32,
        singleton_inner_puzzle_hash: Bytes32,
        delegated_spend: Spend,
    ) -> Result<Spend, DriverError> {
        let puzzle = self.construct_puzzle(ctx)?;
        let solution = self.construct_solution(
            ctx,
            P2DelegatedSingletonSolution {
                singleton_inner_puzzle_hash,
                coin_id,
                delegated_puzzle: delegated_spend.puzzle,
                delegated_solution: delegated_spend.solution,
            },
        )?;
        Ok(Spend { puzzle, solution })
    }

    pub fn spend_coin(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        singleton_inner_puzzle_hash: Bytes32,
        delegated_spend: Spend,
    ) -> Result<(), DriverError> {
        let coin_spend = self.construct_coin_spend(
            ctx,
            coin,
            P2DelegatedSingletonSolution {
                singleton_inner_puzzle_hash,
                coin_id: coin.coin_id(),
                delegated_puzzle: delegated_spend.puzzle,
                delegated_solution: delegated_spend.solution,
            },
        )?;
        ctx.insert(coin_spend);
        Ok(())
    }
}

impl Layer for P2DelegatedSingletonLayer {
    type Solution = P2DelegatedSingletonSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_DELEGATED_SINGLETON_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2DelegatedSingletonArgs::from_clvm(allocator, puzzle.args)?;

        if args.singleton_mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH.into()
            || args.launcher_puzzle_hash != SINGLETON_LAUNCHER_PUZZLE_HASH.into()
        {
            return Err(DriverError::InvalidSingletonStruct);
        }

        Ok(Some(Self {
            launcher_id: args.launcher_id,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2DelegatedSingletonSolution::from_clvm(
            allocator, solution,
        )?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2DelegatedSingletonArgs::new(self.launcher_id))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

impl ToTreeHash for P2DelegatedSingletonLayer {
    fn tree_hash(&self) -> TreeHash {
        P2DelegatedSingletonArgs::curry_tree_hash(self.launcher_id)
    }
}
