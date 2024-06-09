#![allow(clippy::missing_const_for_fn)]

use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::singleton::{
    LauncherSolution, SingletonArgs, SINGLETON_LAUNCHER_PUZZLE, SINGLETON_TOP_LAYER_PUZZLE_HASH,
};
use clvm_traits::{clvm_list, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{Conditions, SpendContext, SpendError};

/// A singleton launcher that is ready to be spent to create the eve singleton. See [`crate::Launcher`] for more information.
#[must_use]
#[derive(Debug, Clone)]
pub struct SpendableLauncher {
    coin: Coin,
    parent: Conditions,
}

impl SpendableLauncher {
    /// Creates a new [`SpendableLauncher`] with the specified launcher coin and parent conditions.
    /// This is used internally by [`crate::Launcher::create`] and [`crate::IntermediateLauncher::create`].
    /// You should not need to use this directly.
    pub fn with_parent_conditions(coin: Coin, parent: Conditions) -> Self {
        Self { coin, parent }
    }

    /// The singleton launcher coin that will be created when the parent is spent.
    #[must_use]
    pub fn coin(&self) -> Coin {
        self.coin
    }

    /// Spends the launcher coin to create the eve singleton.
    /// Includes an optional metadata value that is traditionally a list of key value pairs.
    pub fn spend<T>(
        mut self,
        ctx: &mut SpendContext<'_>,
        singleton_inner_puzzle_hash: Bytes32,
        key_value_list: T,
    ) -> Result<(Conditions, Coin), SpendError>
    where
        T: ToClvm<NodePtr>,
    {
        let singleton_puzzle_hash = CurriedProgram {
            program: SINGLETON_TOP_LAYER_PUZZLE_HASH,
            args: SingletonArgs::new(
                self.coin.coin_id(),
                TreeHash::from(singleton_inner_puzzle_hash),
            ),
        }
        .tree_hash()
        .into();

        let eve_message = ctx.alloc(&clvm_list!(
            singleton_puzzle_hash,
            self.coin.amount,
            &key_value_list
        ))?;
        let eve_message_hash = ctx.tree_hash(eve_message);

        self.parent = self
            .parent
            .assert_coin_announcement(self.coin.coin_id(), eve_message_hash);

        let solution = ctx.serialize(&LauncherSolution {
            singleton_puzzle_hash,
            amount: self.coin.amount,
            key_value_list,
        })?;

        ctx.insert_coin_spend(CoinSpend::new(
            self.coin,
            Program::from(SINGLETON_LAUNCHER_PUZZLE.to_vec()),
            solution,
        ));

        let singleton_coin =
            Coin::new(self.coin.coin_id(), singleton_puzzle_hash, self.coin.amount);

        Ok((self.parent, singleton_coin))
    }
}
