use chia_protocol::{Bytes32, Coin};
use chia_puzzles::singleton::SINGLETON_LAUNCHER_PUZZLE_HASH;

use crate::{ChainedSpend, CreateCoinWithoutMemos, SpendContext, SpendError, SpendableLauncher};

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
        let amount = self.coin.amount;

        Ok(SpendableLauncher::new(
            self.coin,
            ChainedSpend {
                parent_conditions: vec![ctx.alloc(CreateCoinWithoutMemos {
                    puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                    amount,
                })?],
            },
        ))
    }
}
