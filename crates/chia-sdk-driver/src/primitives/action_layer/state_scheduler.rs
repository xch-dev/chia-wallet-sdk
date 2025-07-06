use chia::{
    clvm_utils::ToTreeHash,
    protocol::{Bytes32, Coin, CoinSpend},
    puzzles::{
        singleton::{LauncherSolution, SingletonArgs, SingletonSolution},
        EveProof, LineageProof, Proof,
    },
};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_wallet_sdk::driver::{DriverError, Layer, Spend, SpendContext};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{serde::node_from_bytes, Allocator, NodePtr};

use crate::{StateSchedulerInfo, StateSchedulerLayerSolution};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateScheduler<S>
where
    S: ToTreeHash + Clone + ToClvm<Allocator> + FromClvm<Allocator>,
{
    pub coin: Coin,
    pub proof: Proof,

    pub info: StateSchedulerInfo<S>,
}

impl<S> StateScheduler<S>
where
    S: ToTreeHash + Clone + ToClvm<Allocator> + FromClvm<Allocator>,
{
    pub fn new(coin: Coin, proof: Proof, info: StateSchedulerInfo<S>) -> Self {
        Self { coin, proof, info }
    }

    pub fn child(&self) -> Option<Self> {
        // check for both self.info.generation and self.info.generation + 1 to be < self.info.state_schedule.len()
        if self.info.generation + 1 >= self.info.state_schedule.len() {
            return None;
        };

        let child_proof = Proof::Lineage(LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        });

        let child_info = self.info.clone().with_generation(self.info.generation + 1);
        let child_inner_puzzle_hash = child_info.inner_puzzle_hash();

        Some(Self {
            coin: Coin::new(
                self.coin.coin_id(),
                SingletonArgs::curry_tree_hash(self.info.launcher_id, child_inner_puzzle_hash)
                    .into(),
                1,
            ),
            proof: child_proof,
            info: child_info,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        other_singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError> {
        let lineage_proof = self.proof;
        let coin = self.coin;

        let layers = self.info.into_layers();

        let puzzle = layers.construct_puzzle(ctx)?;
        let solution = layers.construct_solution(
            ctx,
            SingletonSolution {
                lineage_proof,
                amount: coin.amount,
                inner_solution: StateSchedulerLayerSolution {
                    other_singleton_inner_puzzle_hash,
                    inner_solution: (),
                },
            },
        )?;

        ctx.spend(coin, Spend::new(puzzle, solution))?;

        Ok(())
    }

    pub fn from_launcher_spend(
        ctx: &mut SpendContext,
        launcher_spend: CoinSpend,
    ) -> Result<Option<Self>, DriverError> {
        if launcher_spend.coin.puzzle_hash != SINGLETON_LAUNCHER_HASH.into() {
            return Ok(None);
        }

        let solution = node_from_bytes(ctx, &launcher_spend.solution)?;
        let solution = ctx.extract::<LauncherSolution<NodePtr>>(solution)?;

        let Some((info, _other_hints)) =
            StateSchedulerInfo::from_launcher_solution::<NodePtr>(ctx, solution)?
        else {
            return Ok(None);
        };

        let new_coin = Coin::new(
            launcher_spend.coin.coin_id(),
            SingletonArgs::curry_tree_hash(info.launcher_id, info.inner_puzzle_hash()).into(),
            1,
        );

        Ok(Some(Self::new(
            new_coin,
            Proof::Eve(EveProof {
                parent_parent_coin_info: launcher_spend.coin.parent_coin_info,
                parent_amount: launcher_spend.coin.amount,
            }),
            info,
        )))
    }
}

#[cfg(test)]
mod tests {
    use chia_puzzle_types::Memos;
    use chia_wallet_sdk::{
        driver::{Launcher, SingletonLayer},
        test::Simulator,
        types::Conditions,
    };
    use clvmr::NodePtr;

    use crate::{CatalogRegistryState, StateSchedulerLauncherHints};

    use super::*;

    fn mock_state(generator: u8) -> CatalogRegistryState {
        CatalogRegistryState {
            cat_maker_puzzle_hash: Bytes32::new([generator; 32]),
            registration_price: generator as u64 * 1000,
        }
    }

    #[test]
    fn test_price_scheduler() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let mut sim = Simulator::new();

        let schedule: Vec<(u32, CatalogRegistryState)> = vec![
            (0, mock_state(0)),
            (1, mock_state(1)),
            (2, mock_state(2)),
            (3, mock_state(3)),
            (4, mock_state(4)),
            (5, mock_state(5)),
            (6, mock_state(6)),
            (7, mock_state(7)),
        ];
        let final_puzzle_hash: Bytes32 = "yakuhito".tree_hash().into();

        // Launch 'other' singleton, which will consume (reveive) the messages
        let other_singleton_inner_puzzle = ctx.alloc(&1)?;
        let other_singleton_inner_puzzle_hash = ctx.tree_hash(other_singleton_inner_puzzle);

        let other_singleton_launcher = sim.new_coin(SINGLETON_LAUNCHER_HASH.into(), 1);
        let other_launcher = Launcher::new(other_singleton_launcher.parent_coin_info, 1);
        let (_conds, mut other_singleton_coin) =
            other_launcher.spend(ctx, other_singleton_inner_puzzle_hash.into(), ())?;

        sim.spend_coins(ctx.take(), &[])?;

        // Launch state scheduler singleton
        let launcher_coin = sim.new_coin(SINGLETON_LAUNCHER_HASH.into(), 1);
        let launcher = Launcher::new(launcher_coin.parent_coin_info, 1);

        let first_coin_info = StateSchedulerInfo::new(
            launcher_coin.coin_id(),
            other_singleton_launcher.coin_id(),
            schedule.clone(),
            0,
            final_puzzle_hash,
        );
        let (_conds, state_scheduler_coin) = launcher.spend(
            ctx,
            first_coin_info.inner_puzzle_hash().into(),
            StateSchedulerLauncherHints {
                my_launcher_id: launcher_coin.coin_id(),
                receiver_singleton_launcher_id: other_singleton_launcher.coin_id(),
                final_puzzle_hash,
                state_schedule: schedule.clone(),
                final_puzzle_hash_hints: NodePtr::NIL,
            },
        )?;

        let spends = ctx.take();
        assert_eq!(spends.len(), 1);
        let state_scheduler_launcher_spend = spends[0].clone();
        ctx.insert(state_scheduler_launcher_spend.clone());

        sim.spend_coins(ctx.take(), &[])?;

        let mut state_scheduler =
            StateScheduler::from_launcher_spend(ctx, state_scheduler_launcher_spend)?.unwrap();
        assert_eq!(state_scheduler.info, first_coin_info);
        assert_eq!(state_scheduler.coin, state_scheduler_coin);

        let mut other_singleton_coin_parent = other_singleton_coin;
        for (index, (block, new_state)) in schedule.iter().enumerate() {
            state_scheduler
                .clone()
                .spend(ctx, other_singleton_inner_puzzle_hash.into())?;

            let spends = ctx.take();
            assert_eq!(spends.len(), 1);
            let state_scheduler_spend = spends[0].clone();
            ctx.insert(state_scheduler_spend.clone());

            let other_singleton = SingletonLayer::<NodePtr>::new(
                other_singleton_launcher.coin_id(),
                other_singleton_inner_puzzle,
            );
            let other_singleton_lp = if index == 0 {
                Proof::Eve(EveProof {
                    parent_parent_coin_info: other_singleton_launcher.parent_coin_info,
                    parent_amount: other_singleton_launcher.amount,
                })
            } else {
                Proof::Lineage(LineageProof {
                    parent_parent_coin_info: other_singleton_coin_parent.parent_coin_info,
                    parent_inner_puzzle_hash: other_singleton_inner_puzzle_hash.into(),
                    parent_amount: other_singleton_coin_parent.amount,
                })
            };
            let state_scheduler_puzzle_hash_ptr = ctx.alloc(&state_scheduler.coin.puzzle_hash)?;
            let other_singleton_inner_solution = ctx.alloc(
                &Conditions::new()
                    .receive_message(
                        18,
                        new_state.tree_hash().to_vec().into(),
                        vec![state_scheduler_puzzle_hash_ptr],
                    )
                    .create_coin(other_singleton_inner_puzzle_hash.into(), 1, Memos::None),
            )?;
            let other_singleton_spend = other_singleton.construct_spend(
                ctx,
                SingletonSolution {
                    lineage_proof: other_singleton_lp,
                    amount: 1,
                    inner_solution: other_singleton_inner_solution,
                },
            )?;

            ctx.spend(other_singleton_coin, other_singleton_spend)?;
            other_singleton_coin_parent = other_singleton_coin;
            other_singleton_coin = Coin::new(
                other_singleton_coin.coin_id(),
                other_singleton_coin.puzzle_hash,
                1,
            );

            sim.spend_coins(ctx.take(), &[])?;

            if index < schedule.len() - 1 {
                state_scheduler = state_scheduler.child().unwrap();

                assert_eq!(state_scheduler.info.state_schedule, schedule);
                assert_eq!(state_scheduler.info.generation, *block as usize + 1);
            } else {
                break;
            }
        }

        Ok(())
    }
}
