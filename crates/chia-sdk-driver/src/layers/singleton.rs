use std::marker::PhantomData;
use std::string::ParseError;

use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::Proof;
use clvmr::{Allocator, NodePtr};

use crate::PuzzleLayer;

#[derive(Debug)]
pub struct SingletonLayer<IP, IS>
where
    IP: PuzzleLayer<IS>,
{
    pub launcher_id: Bytes32,
    pub inner_puzzle: IP,
    _marker: PhantomData<IS>,
}

#[derive(Debug)]

pub struct SingletonLayerSolution<I> {
    pub lineage_proof: Proof,
    pub inner_solution: I,
}

impl<IP, IS> PuzzleLayer<SingletonLayerSolution<IS>> for SingletonLayer<IP, IS>
where
    IP: PuzzleLayer<IS>,
{
    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        todo!("todo");
    }

    fn from_puzzle(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        todo!("todo");
    }

    fn construct_puzzle(&self, allocator: &mut Allocator) -> NodePtr {
        todo!("todo");
    }

    fn solve(
        &self,
        allocator: &mut Allocator,
        coin: Coin,
        solution: SingletonLayerSolution<IS>,
    ) -> CoinSpend {
        todo!("todo");
    }
}
