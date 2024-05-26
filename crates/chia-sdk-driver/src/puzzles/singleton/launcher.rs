#![allow(clippy::missing_const_for_fn)]

use chia_protocol::{Bytes32, Coin};
use chia_puzzles::singleton::SINGLETON_LAUNCHER_PUZZLE_HASH;

use crate::{
    spend_builder::{P2Spend, ParentConditions},
    SpendContext, SpendError,
};

use super::SpendableLauncher;

#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct Launcher {
    coin: Coin,
}

impl Launcher {
    pub fn new(parent_coin_id: Bytes32, amount: u64) -> Self {
        Self {
            coin: Coin::new(
                parent_coin_id,
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
            ),
        }
    }

    pub fn coin(&self) -> Coin {
        self.coin
    }

    pub fn create(self, ctx: &mut SpendContext<'_>) -> Result<SpendableLauncher, SpendError> {
        Ok(SpendableLauncher::with_parent(
            self.coin,
            ParentConditions::new().create_coin(
                ctx,
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                self.coin.amount,
            )?,
        ))
    }

    pub fn create_from_intermediate(
        self,
        ctx: &mut SpendContext<'_>,
    ) -> Result<(ParentConditions, SpendableLauncher), SpendError> {
        let parent = ParentConditions::new().create_coin(
            ctx,
            SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
            self.coin.amount,
        )?;

        Ok((
            parent,
            SpendableLauncher::with_parent(self.coin, ParentConditions::new()),
        ))
    }
}
