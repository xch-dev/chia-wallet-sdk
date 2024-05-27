#![allow(clippy::missing_const_for_fn)]

use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::singleton::{
    LauncherSolution, SingletonArgs, SingletonStruct, SINGLETON_LAUNCHER_PUZZLE,
    SINGLETON_TOP_LAYER_PUZZLE_HASH,
};
use clvm_traits::{clvm_list, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    spend_builder::{P2Spend, SpendConditions},
    SpendContext, SpendError,
};

#[must_use = "Launcher coins must be spent in order to create the singleton output."]
#[derive(Debug, Clone)]
pub struct SpendableLauncher {
    coin: Coin,
    parent: SpendConditions,
}

impl SpendableLauncher {
    pub fn with_parent(coin: Coin, parent: SpendConditions) -> Self {
        Self { coin, parent }
    }

    #[must_use]
    pub fn coin(&self) -> Coin {
        self.coin
    }

    pub fn spend<T>(
        mut self,
        ctx: &mut SpendContext<'_>,
        singleton_inner_puzzle_hash: Bytes32,
        key_value_list: T,
    ) -> Result<(SpendConditions, Coin), SpendError>
    where
        T: ToClvm<NodePtr>,
    {
        let singleton_puzzle_hash = CurriedProgram {
            program: SINGLETON_TOP_LAYER_PUZZLE_HASH,
            args: SingletonArgs {
                singleton_struct: SingletonStruct::new(self.coin.coin_id()),
                inner_puzzle: TreeHash::from(singleton_inner_puzzle_hash),
            },
        }
        .tree_hash()
        .into();

        let eve_message = ctx.alloc(&clvm_list!(
            singleton_puzzle_hash,
            self.coin.amount,
            &key_value_list
        ))?;
        let eve_message_hash = ctx.tree_hash(eve_message);

        self.parent =
            self.parent
                .assert_coin_announcement(ctx, self.coin.coin_id(), eve_message_hash)?;

        let solution = ctx.serialize(&LauncherSolution {
            singleton_puzzle_hash,
            amount: self.coin.amount,
            key_value_list,
        })?;

        ctx.spend(CoinSpend::new(
            self.coin,
            Program::from(SINGLETON_LAUNCHER_PUZZLE.to_vec()),
            solution,
        ));

        let singleton_coin =
            Coin::new(self.coin.coin_id(), singleton_puzzle_hash, self.coin.amount);

        Ok((self.parent, singleton_coin))
    }
}
