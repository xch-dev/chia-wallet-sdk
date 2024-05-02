use chia_protocol::CoinSpend;
use chia_wallet::offer::SETTLEMENT_PAYMENTS_PUZZLE_HASH;
use clvm_traits::ToClvm;
use clvmr::NodePtr;

use crate::{CreateCoinWithMemos, CreateCoinWithoutMemos, SpendError};

pub trait BaseSpend {
    fn chain(self, chained_spend: ChainedSpend) -> Self;
    fn condition(self, condition: impl ToClvm<NodePtr>) -> Result<Self, SpendError>
    where
        Self: Sized;

    fn conditions(
        mut self,
        conditions: impl IntoIterator<Item = impl ToClvm<NodePtr>>,
    ) -> Result<Self, SpendError>
    where
        Self: Sized,
    {
        for condition in conditions {
            self = self.condition(condition)?;
        }
        Ok(self)
    }

    fn unhinted_settlement_coin(self, amount: u64) -> Result<Self, SpendError>
    where
        Self: Sized,
    {
        self.condition(CreateCoinWithoutMemos {
            puzzle_hash: SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
            amount,
        })
    }

    fn settlement_coin(self, amount: u64) -> Result<Self, SpendError>
    where
        Self: Sized,
    {
        self.condition(CreateCoinWithMemos {
            puzzle_hash: SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
            amount,
            memos: vec![SETTLEMENT_PAYMENTS_PUZZLE_HASH.to_vec().into()],
        })
    }
}

#[must_use = "The contents of a chained spend must be used."]
#[derive(Debug, Clone)]
pub struct ChainedSpend {
    pub coin_spends: Vec<CoinSpend>,
    pub parent_conditions: Vec<NodePtr>,
}
