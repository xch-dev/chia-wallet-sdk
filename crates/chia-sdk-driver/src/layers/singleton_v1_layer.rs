use chia_protocol::Bytes32;
use chia_puzzle_types::{Proof, singleton::SingletonSolution};
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_HASH};
use chia_sdk_types::{Mod, puzzles::SingletonV1Args};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The [`SingletonLayer`](crate::SingletonLayer) equivalent for the original singleton top layer,
/// which is only used by plot NFTs. The two versions have different mod hashes, so neither layer
/// can parse or construct the other's puzzles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SingletonV1Layer<I> {
    /// The unique launcher id for the singleton. Also referred to as the singleton id.
    pub launcher_id: Bytes32,
    /// The inner puzzle layer, which determines the actual behavior of the coin.
    pub inner_puzzle: I,
}

impl<I> SingletonV1Layer<I> {
    pub fn new(launcher_id: Bytes32, inner_puzzle: I) -> Self {
        Self {
            launcher_id,
            inner_puzzle,
        }
    }
}

impl<I> Layer for SingletonV1Layer<I>
where
    I: Layer,
{
    type Solution = SingletonSolution<I::Solution>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != SINGLETON_TOP_LAYER_HASH.into() {
            return Ok(None);
        }

        let args = SingletonV1Args::<NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.singleton_struct.mod_hash != SINGLETON_TOP_LAYER_HASH.into()
            || args.singleton_struct.launcher_puzzle_hash != SINGLETON_LAUNCHER_HASH.into()
        {
            return Err(DriverError::InvalidSingletonStruct);
        }

        let Some(inner_puzzle) =
            I::parse_puzzle(allocator, Puzzle::parse(allocator, args.inner_puzzle))?
        else {
            return Ok(None);
        };

        Ok(Some(Self {
            launcher_id: args.singleton_struct.launcher_id,
            inner_puzzle,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        // The puzzle ignores anything after the inner solution
        let (lineage_proof, (amount, (inner_solution, _))) =
            <(Proof, (u64, (NodePtr, NodePtr)))>::from_clvm(allocator, solution)?;

        Ok(SingletonSolution {
            lineage_proof,
            amount,
            inner_solution: I::parse_solution(allocator, inner_solution)?,
        })
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        ctx.curry(SingletonV1Args::new(self.launcher_id, inner_puzzle))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let inner_solution = self
            .inner_puzzle
            .construct_solution(ctx, solution.inner_solution)?;
        ctx.alloc(&SingletonSolution {
            lineage_proof: solution.lineage_proof,
            amount: solution.amount,
            inner_solution,
        })
    }
}

impl<I> ToTreeHash for SingletonV1Layer<I>
where
    I: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        SingletonV1Args::new(self.launcher_id, self.inner_puzzle.tree_hash()).curry_tree_hash()
    }
}
