use std::string::ParseError;

use chia_protocol::{Coin, CoinSpend};
use clvmr::{Allocator, NodePtr};

pub trait PuzzleLayer<S>
where
    Self: Sized,
{
    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, ParseError>;
    fn from_puzzle(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
    ) -> Result<Option<Self>, ParseError>;
    fn construct_puzzle(&self, allocator: &mut Allocator) -> NodePtr;
    fn solve(&self, allocator: &mut Allocator, coin: Coin, solution: S) -> CoinSpend;
}
