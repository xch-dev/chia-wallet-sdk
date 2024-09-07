#![allow(clippy::missing_const_for_fn)]

use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::singleton::{
    LauncherSolution, SingletonArgs, SINGLETON_LAUNCHER_PUZZLE, SINGLETON_LAUNCHER_PUZZLE_HASH,
};
use chia_sdk_types::{announcement_id, Conditions};
use clvm_traits::ToClvm;
use clvmr::Allocator;

use crate::{DriverError, SpendContext};

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
            Conditions::new().create_coin(
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
                Vec::new(),
            ),
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
            Conditions::new().create_coin(
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
                vec![hint.into()],
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
            Conditions::new().create_coin(
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
                Vec::new(),
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
            Conditions::new().create_coin(
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
                vec![hint.into()],
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
        ctx: &mut SpendContext,
        singleton_inner_puzzle_hash: Bytes32,
        key_value_list: T,
    ) -> Result<(Conditions, Coin), DriverError>
    where
        T: ToClvm<Allocator>,
    {
        let amount = self.coin.amount;
        self.spend_with_amount(ctx, singleton_inner_puzzle_hash, amount, key_value_list)
    }

    /// Spends the launcher coin to create the eve singleton with a custom amount.
    /// This amount should be the same as the launcher's amount in most cases.
    /// Includes an optional metadata value that is traditionally a list of key value pairs.
    pub fn spend_with_amount<T>(
        self,
        ctx: &mut SpendContext,
        singleton_inner_puzzle_hash: Bytes32,
        singleton_amount: u64,
        key_value_list: T,
    ) -> Result<(Conditions, Coin), DriverError>
    where
        T: ToClvm<Allocator>,
    {
        let singleton_puzzle_hash =
            SingletonArgs::curry_tree_hash(self.coin.coin_id(), singleton_inner_puzzle_hash.into())
                .into();

        let solution_ptr = ctx.alloc(&LauncherSolution {
            singleton_puzzle_hash,
            amount: singleton_amount,
            key_value_list,
        })?;

        let solution = ctx.serialize(&solution_ptr)?;

        ctx.insert(CoinSpend::new(
            self.coin,
            Program::from(SINGLETON_LAUNCHER_PUZZLE.to_vec()),
            solution,
        ));

        let singleton_coin =
            Coin::new(self.coin.coin_id(), singleton_puzzle_hash, singleton_amount);

        Ok((
            self.conditions.assert_coin_announcement(announcement_id(
                self.coin.coin_id(),
                ctx.tree_hash(solution_ptr),
            )),
            singleton_coin,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::StandardLayer;

    use super::*;

    use chia_sdk_test::Simulator;

    #[test]
    fn test_singleton_launcher() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let (sk, pk, _puzzle_hash, coin) = sim.new_p2(1)?;

        let ctx = &mut SpendContext::new();
        let launcher = Launcher::new(coin.coin_id(), 1);
        assert_eq!(launcher.coin.amount, 1);

        let (conditions, singleton) = launcher.spend(ctx, Bytes32::default(), ())?;
        StandardLayer::new(pk).spend(ctx, coin, conditions)?;
        assert_eq!(singleton.amount, 1);

        sim.spend_coins(ctx.take(), &[sk])?;

        Ok(())
    }

    #[test]
    fn test_singleton_launcher_custom_amount() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let (sk, pk, _puzzle_hash, coin) = sim.new_p2(1)?;

        let ctx = &mut SpendContext::new();
        let launcher = Launcher::new(coin.coin_id(), 0);
        assert_eq!(launcher.coin.amount, 0);

        let (conditions, singleton) = launcher.spend_with_amount(ctx, Bytes32::default(), 1, ())?;
        StandardLayer::new(pk).spend(ctx, coin, conditions)?;
        assert_eq!(singleton.amount, 1);

        sim.spend_coins(ctx.take(), &[sk])?;

        Ok(())
    }
}
