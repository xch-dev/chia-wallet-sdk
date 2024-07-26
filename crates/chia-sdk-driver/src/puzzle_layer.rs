use chia_protocol::{Coin, CoinSpend};
use clvmr::{Allocator, NodePtr};

pub trait PuzzleLayer<P, S>
where
    Self: Sized,
{
    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Option<Self>;
    fn from_puzzle(allocator: &mut Allocator, layer_puzzle: NodePtr) -> Option<Self>;
    fn construct_puzzle(&self, allocator: &mut Allocator) -> NodePtr;
    fn solve(&self, allocator: &mut Allocator, coin: Coin, solution: S) -> CoinSpend;
}
