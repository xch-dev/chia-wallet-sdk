use chia_protocol::CoinSpend;
use clvm_traits::ToClvm;
use clvmr::NodePtr;

use crate::SpendError;

pub trait BaseSpend {
    fn chain(self, chained_spend: ChainedSpend) -> Self;
    fn condition(self, condition: impl ToClvm<NodePtr>) -> Result<Self, SpendError>
    where
        Self: Sized;
}

#[must_use = "The contents of a chained spend must be used."]
pub struct ChainedSpend {
    pub coin_spends: Vec<CoinSpend>,
    pub parent_conditions: Vec<NodePtr>,
}
