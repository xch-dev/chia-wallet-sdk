use chia_protocol::{Coin, CoinSpend};
use clvmr::{Allocator, NodePtr};

use crate::{ParseError, SpendContext};

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

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, ParseError>;

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: S,
    ) -> Result<NodePtr, ParseError>;
}

pub trait OuterPuzzleLayer<S>
where
    Self: Sized,
{
    fn solve(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        solution: S,
    ) -> Result<CoinSpend, ParseError>;
}
