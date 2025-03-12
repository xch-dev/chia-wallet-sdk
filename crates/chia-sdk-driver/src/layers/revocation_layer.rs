use chia_protocol::Bytes32;
use chia_puzzles::REVOCATION_LAYER_HASH;
use chia_sdk_types::puzzles::{RevocationArgs, RevocationSolution};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The revocation [`Layer`] allows the issuer to revoke the asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevocationLayer {
    pub hidden_puzzle_hash: Bytes32,
    pub inner_puzzle_hash: Bytes32,
}

impl RevocationLayer {
    pub fn new(hidden_puzzle_hash: Bytes32, inner_puzzle_hash: Bytes32) -> Self {
        Self {
            hidden_puzzle_hash,
            inner_puzzle_hash,
        }
    }
}

impl Layer for RevocationLayer {
    type Solution = RevocationSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != REVOCATION_LAYER_HASH.into() {
            return Ok(None);
        }

        let args = RevocationArgs::from_clvm(allocator, puzzle.args)?;

        if args.mod_hash != REVOCATION_LAYER_HASH.into() {
            return Err(DriverError::InvalidModHash);
        }

        Ok(Some(Self {
            hidden_puzzle_hash: args.hidden_puzzle_hash,
            inner_puzzle_hash: args.inner_puzzle_hash,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(RevocationSolution::<NodePtr, NodePtr>::from_clvm(
            allocator, solution,
        )?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(RevocationArgs::new(
            self.hidden_puzzle_hash,
            self.inner_puzzle_hash,
        ))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

impl ToTreeHash for RevocationLayer {
    fn tree_hash(&self) -> TreeHash {
        RevocationArgs::new(self.hidden_puzzle_hash, self.inner_puzzle_hash).tree_hash()
    }
}
