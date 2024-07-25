use chia_protocol::{Coin, CoinSpend};
use clvmr::{Allocator, NodePtr};

use crate::ParseError;

// given a spend, will return info about the coin being created
pub trait FromSpend<A = ()>
where
    Self: Sized,
{
    fn from_spend(
        allocator: &mut Allocator,
        coin: Coin,
        puzzle: NodePtr,
        solution: NodePtr,
        additional_info: A,
    ) -> Result<Self, ParseError>;

    fn from_coin_spend(
        allocator: &mut Allocator,
        cs: &CoinSpend,
        additional_info: A,
    ) -> Result<Self, ParseError>;
}
