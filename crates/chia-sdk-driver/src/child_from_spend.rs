use chia_protocol::CoinSpend;
use clvmr::Allocator;
use thiserror::Error;

// given a spend, will return info about the coin being spent
pub trait ChildFromSpend<R, A> {
    fn from_spend(
        allocator: &mut Allocator,
        cs: &CoinSpend,
        additional_info: A,
    ) -> Result<R, FromSpendError>;
}
