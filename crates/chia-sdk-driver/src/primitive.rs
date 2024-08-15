use chia_protocol::Coin;
use clvmr::{Allocator, NodePtr};

use crate::DriverError;

pub trait Primitive {
    fn from_parent_spend(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: NodePtr,
        parent_solution: NodePtr,
        coin: Coin,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized;
}
