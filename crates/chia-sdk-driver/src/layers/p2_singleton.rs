use chia_protocol::{Bytes32, Coin};
use chia_puzzles::singleton::{SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH};
use chia_sdk_types::{P2SingletonArgs, P2SingletonSolution, P2_SINGLETON_PUZZLE_HASH};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

/// The p2 singleton [`Layer`] allows for requiring that a
/// singleton be spent alongside this coin to authorize it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2Singleton {
    pub launcher_id: Bytes32,
}

impl P2Singleton {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self { launcher_id }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        coin_id: Bytes32,
        singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<Spend, DriverError> {
        let puzzle = self.construct_puzzle(ctx)?;
        let solution = self.construct_solution(
            ctx,
            P2SingletonSolution {
                singleton_inner_puzzle_hash,
                my_id: coin_id,
            },
        )?;
        Ok(Spend { puzzle, solution })
    }

    pub fn spend_coin(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError> {
        let coin_spend = self.construct_coin_spend(
            ctx,
            coin,
            P2SingletonSolution {
                singleton_inner_puzzle_hash,
                my_id: coin.coin_id(),
            },
        )?;
        ctx.insert(coin_spend);
        Ok(())
    }
}

impl Layer for P2Singleton {
    type Solution = P2SingletonSolution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_SINGLETON_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2SingletonArgs::from_clvm(allocator, puzzle.args)?;

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
        Ok(P2SingletonSolution::from_clvm(allocator, solution)?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2SingletonArgs::new(self.launcher_id))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

impl ToTreeHash for P2Singleton {
    fn tree_hash(&self) -> TreeHash {
        P2SingletonArgs::curry_tree_hash(self.launcher_id)
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::Coin;
    use chia_puzzles::{singleton::SingletonSolution, EveProof, Proof};
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;

    use super::*;

    use crate::{Launcher, SingletonLayer, SpendWithConditions, StandardLayer};

    #[test]
    fn test_p2_singleton_layer() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let (sk, pk, puzzle_hash, coin) = sim.new_p2(2)?;
        let p2 = StandardLayer::new(pk);

        let launcher = Launcher::new(coin.coin_id(), 1);
        let launcher_id = launcher.coin().coin_id();
        let (create_singleton, singleton) = launcher.spend(ctx, puzzle_hash, ())?;

        let p2_singleton = P2Singleton::new(launcher_id);
        let p2_singleton_hash = p2_singleton.tree_hash().into();

        p2.spend(
            ctx,
            coin,
            create_singleton.create_coin(p2_singleton_hash, 1, vec![launcher_id.into()]),
        )?;

        let p2_coin = Coin::new(coin.coin_id(), p2_singleton_hash, 1);
        p2_singleton.spend_coin(ctx, p2_coin, puzzle_hash)?;

        let inner_solution = p2
            .spend_with_conditions(
                ctx,
                Conditions::new()
                    .create_coin(puzzle_hash, 1, vec![launcher_id.into()])
                    .create_puzzle_announcement(p2_coin.coin_id().into()),
            )?
            .solution;
        let singleton_spend = SingletonLayer::new(launcher_id, p2.construct_puzzle(ctx)?)
            .construct_coin_spend(
                ctx,
                singleton,
                SingletonSolution {
                    lineage_proof: Proof::Eve(EveProof {
                        parent_parent_coin_info: coin.coin_id(),
                        parent_amount: 1,
                    }),
                    amount: singleton.amount,
                    inner_solution,
                },
            )?;
        ctx.insert(singleton_spend);

        sim.spend_coins(ctx.take(), &[sk])?;

        Ok(())
    }
}
