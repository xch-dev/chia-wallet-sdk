#![allow(clippy::missing_const_for_fn)]

use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::singleton::{
    LauncherSolution, SingletonArgs, SINGLETON_LAUNCHER_PUZZLE, SINGLETON_LAUNCHER_PUZZLE_HASH,
};
use clvm_traits::ToClvm;
use clvmr::NodePtr;

use crate::{Conditions, SpendContext, SpendError};

/// A singleton launcher is a coin that is spent within the same block to create a singleton.
/// The first coin that is created is known as an "eve" singleton.
/// The [`Launcher`] type allows you to get the launcher id before committing to creating the singleton.
#[derive(Debug, Clone)]
#[must_use]
pub struct Launcher {
    coin: Coin,
    conditions: Conditions,
}

impl Launcher {
    /// Creates a new [`Launcher`] with the specified launcher coin and parent spend conditions.
    pub fn from_coin(coin: Coin, conditions: Conditions) -> Self {
        Self { coin, conditions }
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// By default, no hint is used when creating the launcher coin. To specify a hint, use [`Launcher::hinted`].
    pub fn new(parent_coin_id: Bytes32, amount: u64) -> Self {
        Self::from_coin(
            Coin::new(
                parent_coin_id,
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
            ),
            Conditions::new().create_coin(SINGLETON_LAUNCHER_PUZZLE_HASH.into(), amount),
        )
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// The created launcher coin will be hinted to make identifying it easier later.
    pub fn hinted(parent_coin_id: Bytes32, amount: u64, hint: Bytes32) -> Self {
        Self::from_coin(
            Coin::new(
                parent_coin_id,
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
            ),
            Conditions::new().create_hinted_coin(
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
                hint,
            ),
        )
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// By default, no hint is used when creating the coin. To specify a hint, use [`Launcher::create_early_hinted`].
    ///
    /// This method is used to create the launcher coin immediately from the parent, then spend it later attached to any coin spend.
    /// For example, this is useful for minting NFTs from intermediate coins created with an earlier instance of a DID.
    pub fn create_early(parent_coin_id: Bytes32, amount: u64) -> (Conditions, Self) {
        (
            Conditions::new().create_coin(SINGLETON_LAUNCHER_PUZZLE_HASH.into(), amount),
            Self::from_coin(
                Coin::new(
                    parent_coin_id,
                    SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                    amount,
                ),
                Conditions::new(),
            ),
        )
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// The created launcher coin will be hinted to make identifying it easier later.
    ///
    /// This method is used to create the launcher coin immediately from the parent, then spend it later attached to any coin spend.
    /// For example, this is useful for minting NFTs from intermediate coins created with an earlier instance of a DID.
    pub fn create_early_hinted(
        parent_coin_id: Bytes32,
        amount: u64,
        hint: Bytes32,
    ) -> (Conditions, Self) {
        (
            Conditions::new().create_hinted_coin(
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
                hint,
            ),
            Self::from_coin(
                Coin::new(
                    parent_coin_id,
                    SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                    amount,
                ),
                Conditions::new(),
            ),
        )
    }

    /// The singleton launcher coin that will be created when the parent is spent.
    pub fn coin(&self) -> Coin {
        self.coin
    }

    /// Spends the launcher coin to create the eve singleton.
    /// Includes an optional metadata value that is traditionally a list of key value pairs.
    pub fn spend<T>(
        self,
        ctx: &mut SpendContext<'_>,
        singleton_inner_puzzle_hash: Bytes32,
        key_value_list: T,
    ) -> Result<(Conditions, Coin), SpendError>
    where
        T: ToClvm<NodePtr>,
    {
        let singleton_puzzle_hash =
            SingletonArgs::curry_tree_hash(self.coin.coin_id(), singleton_inner_puzzle_hash.into())
                .into();

        let solution_ptr = ctx.alloc(&LauncherSolution {
            singleton_puzzle_hash,
            amount: self.coin.amount,
            key_value_list,
        })?;

        let solution = ctx.serialize(&solution_ptr)?;

        ctx.insert_coin_spend(CoinSpend::new(
            self.coin,
            Program::from(SINGLETON_LAUNCHER_PUZZLE.to_vec()),
            solution,
        ));

        let singleton_coin =
            Coin::new(self.coin.coin_id(), singleton_puzzle_hash, self.coin.amount);

        Ok((
            self.conditions
                .assert_coin_announcement(self.coin.coin_id(), ctx.tree_hash(solution_ptr)),
            singleton_coin,
        ))
    }
}

/*
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
 */
