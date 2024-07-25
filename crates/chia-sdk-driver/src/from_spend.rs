use chia_protocol::CoinSpend;
use clvmr::Allocator;
use thiserror::Error;

#[derive(Debug, Error, Clone, Copy)]
pub enum FromSpendError {
    #[error("placeholder error")]
    Placeholder,
}

pub trait FromSpend<N> {
    fn from_spend(
        allocator: &mut Allocator,
        cs: &CoinSpend,
        prev_state_info: N,
    ) -> Result<(), FromSpendError>;
}
