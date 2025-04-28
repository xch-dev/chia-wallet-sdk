use chia_protocol::Bytes32;
use chia_sdk_types::{
    puzzles::{OptionContractArgs, OptionContractSolution, OPTION_CONTRACT_PUZZLE_HASH},
    Mod,
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The option contract [`Layer`] keeps track of the underlying coin and right to exercise the option.
/// It's typically an inner layer of the [`SingletonLayer`](crate::SingletonLayer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionContractLayer<I> {
    pub underlying_coin_id: Bytes32,
    pub underlying_delegated_puzzle_hash: Bytes32,
    pub inner_puzzle: I,
}

impl<I> OptionContractLayer<I> {
    pub fn new(
        underlying_coin_id: Bytes32,
        underlying_delegated_puzzle_hash: Bytes32,
        inner_puzzle: I,
    ) -> Self {
        Self {
            underlying_coin_id,
            underlying_delegated_puzzle_hash,
            inner_puzzle,
        }
    }
}

impl<I> Layer for OptionContractLayer<I>
where
    I: Layer,
    I::Solution: FromClvm<Allocator>,
{
    type Solution = OptionContractSolution<I::Solution>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != OPTION_CONTRACT_PUZZLE_HASH {
            return Ok(None);
        }

        let args = OptionContractArgs::<NodePtr>::from_clvm(allocator, puzzle.args)?;

        let Some(inner_puzzle) =
            I::parse_puzzle(allocator, Puzzle::parse(allocator, args.inner_puzzle))?
        else {
            return Ok(None);
        };

        Ok(Some(Self {
            underlying_coin_id: args.underlying_coin_id,
            underlying_delegated_puzzle_hash: args.underlying_delegated_puzzle_hash,
            inner_puzzle,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(Self::Solution::from_clvm(allocator, solution)?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        ctx.curry(OptionContractArgs::new(
            self.underlying_coin_id,
            self.underlying_delegated_puzzle_hash,
            inner_puzzle,
        ))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let inner_solution = self
            .inner_puzzle
            .construct_solution(ctx, solution.inner_solution)?;
        ctx.alloc(&OptionContractSolution::new(inner_solution))
    }
}

impl<I> ToTreeHash for OptionContractLayer<I>
where
    I: ToTreeHash,
    I: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        let inner_puzzle_hash = self.inner_puzzle.tree_hash();
        OptionContractArgs::new(
            self.underlying_coin_id,
            self.underlying_delegated_puzzle_hash,
            inner_puzzle_hash,
        )
        .curry_tree_hash()
    }
}
