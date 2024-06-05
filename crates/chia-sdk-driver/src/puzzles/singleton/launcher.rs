#![allow(clippy::missing_const_for_fn)]

use chia_protocol::{Bytes32, Coin};
use chia_puzzles::singleton::SINGLETON_LAUNCHER_PUZZLE_HASH;

use crate::{
    spend_builder::{P2Spend, SpendConditions},
    SpendContext, SpendError,
};

use super::SpendableLauncher;

/// A singleton launcher is a coin that is spent within the same block to create a singleton.
/// The first coin that is created is known as an "eve" singleton.
/// The [`Launcher`] type allows you to get the launcher id before committing to creating the singleton,
/// but to prevent misuse it's impossible to create the launcher coin without also spending it.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct Launcher {
    coin: Coin,
}

impl Launcher {
    /// Starts the singleton launch process, but defers creating the launcher coin until it's actually spent.
    pub fn new(parent_coin_id: Bytes32, amount: u64) -> Self {
        Self {
            coin: Coin::new(
                parent_coin_id,
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
            ),
        }
    }

    /// The singleton launcher coin that will be created when the parent is spent.
    pub fn coin(&self) -> Coin {
        self.coin
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// By default, no hint is used when creating the coin. To specify a hint, use [`Launcher::create_hinted`].
    pub fn create(self, ctx: &mut SpendContext<'_>) -> Result<SpendableLauncher, SpendError> {
        Ok(SpendableLauncher::with_parent_conditions(
            self.coin,
            SpendConditions::new().create_coin(
                ctx,
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                self.coin.amount,
            )?,
        ))
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// The created launcher coin will be hinted to make identifying it easier later.
    pub fn create_hinted(
        self,
        ctx: &mut SpendContext<'_>,
        hint: Bytes32,
    ) -> Result<SpendableLauncher, SpendError> {
        Ok(SpendableLauncher::with_parent_conditions(
            self.coin,
            SpendConditions::new().create_hinted_coin(
                ctx,
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                self.coin.amount,
                hint,
            )?,
        ))
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// By default, no hint is used when creating the coin. To specify a hint, use [`Launcher::create_hinted_now`].
    ///
    /// This method is used to create the launcher coin immediately from the parent, then spend it later attached to any coin spend.
    /// For example, this is useful for minting NFTs from intermediate coins created with an earlier instance of a DID.
    pub fn create_now(
        self,
        ctx: &mut SpendContext<'_>,
    ) -> Result<(SpendConditions, SpendableLauncher), SpendError> {
        Ok((
            SpendConditions::new().create_coin(
                ctx,
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                self.coin.amount,
            )?,
            SpendableLauncher::with_parent_conditions(self.coin, SpendConditions::new()),
        ))
    }

    /// The parent coin specified when constructing the [`Launcher`] will create the launcher coin.
    /// The created launcher coin will be hinted to make identifying it easier later.
    ///
    /// This method is used to create the launcher coin immediately from the parent, then spend it later attached to any coin spend.
    /// For example, this is useful for minting NFTs from intermediate coins created with an earlier instance of a DID.
    pub fn create_hinted_now(
        self,
        ctx: &mut SpendContext<'_>,
        hint: Bytes32,
    ) -> Result<(SpendConditions, SpendableLauncher), SpendError> {
        Ok((
            SpendConditions::new().create_hinted_coin(
                ctx,
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                self.coin.amount,
                hint,
            )?,
            SpendableLauncher::with_parent_conditions(self.coin, SpendConditions::new()),
        ))
    }
}
