use chia_protocol::Bytes32;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_sdk_types::puzzles::{
    P2DelegatedBySingletonLayerArgs, P2DelegatedBySingletonLayerSolution,
    P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct P2DelegatedBySingletonLayer {
    pub singleton_struct_hash: Bytes32,
    pub nonce: u64,
}

impl P2DelegatedBySingletonLayer {
    pub fn new(singleton_struct_hash: Bytes32, nonce: u64) -> Self {
        Self {
            singleton_struct_hash,
            nonce,
        }
    }
}

impl Layer for P2DelegatedBySingletonLayer {
    type Solution = P2DelegatedBySingletonLayerSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2DelegatedBySingletonLayerArgs::from_clvm(allocator, puzzle.args)?;

        if args.singleton_mod_hash != SINGLETON_TOP_LAYER_V1_1_HASH.into() {
            return Ok(None);
        }

        Ok(Some(Self {
            singleton_struct_hash: args.singleton_struct_hash,
            nonce: args.nonce,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        P2DelegatedBySingletonLayerSolution::from_clvm(allocator, solution)
            .map_err(DriverError::FromClvm)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2DelegatedBySingletonLayerArgs::new(
            self.singleton_struct_hash,
            self.nonce,
        ))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        solution.to_clvm(ctx).map_err(DriverError::ToClvm)
    }
}
