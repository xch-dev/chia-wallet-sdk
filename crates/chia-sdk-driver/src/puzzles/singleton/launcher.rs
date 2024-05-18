use chia_protocol::{Bytes32, Coin};
use chia_puzzles::singleton::SINGLETON_LAUNCHER_PUZZLE_HASH;
use chia_sdk_types::conditions::CreateCoinWithoutMemos;

use crate::{spend_builder::ChainedSpend, SpendContext, SpendError};

use super::SpendableLauncher;

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

    pub fn create(self, ctx: &mut SpendContext) -> Result<SpendableLauncher, SpendError> {
        Ok(SpendableLauncher::new(
            self.coin,
            ChainedSpend {
                parent_conditions: vec![ctx.alloc(CreateCoinWithoutMemos {
                    puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                    amount: self.coin.amount,
                })?],
            },
        ))
    }

    pub fn create_from_intermediate(
        self,
        ctx: &mut SpendContext,
    ) -> Result<(ChainedSpend, SpendableLauncher), SpendError> {
        let chained_spend = ChainedSpend {
            parent_conditions: vec![ctx.alloc(CreateCoinWithoutMemos {
                puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount: self.coin.amount,
            })?],
        };
        Ok((
            chained_spend,
            SpendableLauncher::new(self.coin, ChainedSpend::default()),
        ))
    }
}
