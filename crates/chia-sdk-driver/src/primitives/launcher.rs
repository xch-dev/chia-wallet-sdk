#![allow(clippy::missing_const_for_fn)]

use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzle_types::singleton::{LauncherSolution, SingletonArgs};
use chia_puzzles::{SINGLETON_LAUNCHER, SINGLETON_LAUNCHER_HASH};
use chia_sdk_types::{
    announcement_id,
    conditions::{CreateCoin, Memos},
    Conditions,
};
use clvm_traits::ToClvm;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, SpendContext};

/// A singleton launcher is a coin that is spent within the same block to create a singleton.
/// The first coin that is created is known as an "eve" singleton.
/// The [`Launcher`] type allows you to get the launcher id before committing to creating the singleton.
#[derive(Debug, Clone)]
#[must_use]
pub struct Launcher {
    coin: Coin,
    conditions: Conditions,
    singleton_amount: u64,
}

impl Launcher {
    /// Creates a new [`Launcher`] with the specified launcher coin and parent spend conditions.
    pub fn from_coin(coin: Coin, conditions: Conditions) -> Self {
        Self {
            coin,
            conditions,
            singleton_amount: coin.amount,
        }
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// By default, no hint is used when creating the launcher coin. To specify a hint, use [`Launcher::with_memos`].
    pub fn new(parent_coin_id: Bytes32, amount: u64) -> Self {
        Self::from_coin(
            Coin::new(parent_coin_id, SINGLETON_LAUNCHER_HASH.into(), amount),
            Conditions::new().create_coin(SINGLETON_LAUNCHER_HASH.into(), amount, Memos::None),
        )
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// The created launcher coin will be hinted to make identifying it easier later.
    pub fn with_memos(parent_coin_id: Bytes32, amount: u64, memos: Memos<NodePtr>) -> Self {
        Self::from_coin(
            Coin::new(parent_coin_id, SINGLETON_LAUNCHER_HASH.into(), amount),
            Conditions::new().create_coin(SINGLETON_LAUNCHER_HASH.into(), amount, memos),
        )
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// By default, no hint is used when creating the coin. To specify a hint, use [`Launcher::create_early_with_memos`].
    ///
    /// This method is used to create the launcher coin immediately from the parent, then spend it later attached to any coin spend.
    /// For example, this is useful for minting NFTs from intermediate coins created with an earlier instance of a DID.
    pub fn create_early(parent_coin_id: Bytes32, amount: u64) -> (CreateCoin<NodePtr>, Self) {
        (
            CreateCoin::new(SINGLETON_LAUNCHER_HASH.into(), amount, Memos::None),
            Self::from_coin(
                Coin::new(parent_coin_id, SINGLETON_LAUNCHER_HASH.into(), amount),
                Conditions::new(),
            ),
        )
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// The created launcher coin will be hinted to make identifying it easier later.
    ///
    /// This method is used to create the launcher coin immediately from the parent, then spend it later attached to any coin spend.
    /// For example, this is useful for minting NFTs from intermediate coins created with an earlier instance of a DID.
    pub fn create_early_with_memos(
        parent_coin_id: Bytes32,
        amount: u64,
        memos: Memos<NodePtr>,
    ) -> (CreateCoin<NodePtr>, Self) {
        (
            CreateCoin::new(SINGLETON_LAUNCHER_HASH.into(), amount, memos),
            Self::from_coin(
                Coin::new(parent_coin_id, SINGLETON_LAUNCHER_HASH.into(), amount),
                Conditions::new(),
            ),
        )
    }

    /// Changes the singleton amount to differ from the launcher amount.
    /// This is useful in situations where the launcher amount is 0 and the singleton amount is 1, for example.
    pub fn with_singleton_amount(mut self, singleton_amount: u64) -> Self {
        self.singleton_amount = singleton_amount;
        self
    }

    /// Returns the current singleton amount.
    pub fn singleton_amount(&self) -> u64 {
        self.singleton_amount
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
        let singleton_puzzle_hash =
            SingletonArgs::curry_tree_hash(self.coin.coin_id(), singleton_inner_puzzle_hash.into())
                .into();

        let solution_ptr = ctx.alloc(&LauncherSolution {
            singleton_puzzle_hash,
            amount: self.singleton_amount,
            key_value_list,
        })?;

        let solution = ctx.serialize(&solution_ptr)?;

        ctx.insert(CoinSpend::new(
            self.coin,
            Program::from(SINGLETON_LAUNCHER.to_vec()),
            solution,
        ));

        let singleton_coin = Coin::new(
            self.coin.coin_id(),
            singleton_puzzle_hash,
            self.singleton_amount,
        );

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

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let ctx = &mut SpendContext::new();
        let launcher = Launcher::new(alice.coin.coin_id(), 1);
        assert_eq!(launcher.coin.amount, 1);

        let (conditions, singleton) = launcher.spend(ctx, Bytes32::default(), ())?;
        alice_p2.spend(ctx, alice.coin, conditions)?;
        assert_eq!(singleton.amount, 1);

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }

    #[test]
    fn test_singleton_launcher_custom_amount() -> anyhow::Result<()> {
        let mut sim = Simulator::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let ctx = &mut SpendContext::new();
        let launcher = Launcher::new(alice.coin.coin_id(), 0);
        assert_eq!(launcher.coin.amount, 0);

        let (conditions, singleton) =
            launcher
                .with_singleton_amount(1)
                .spend(ctx, Bytes32::default(), ())?;
        alice_p2.spend(ctx, alice.coin, conditions)?;
        assert_eq!(singleton.amount, 1);

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }
}
