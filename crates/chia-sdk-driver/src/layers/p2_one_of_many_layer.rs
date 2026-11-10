use chia_protocol::Bytes32;
use chia_puzzles::P2_1_OF_N_HASH;
use chia_sdk_types::puzzles::{P2OneOfManyArgs, P2OneOfManySolution};
use clvm_traits::FromClvm;
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The p2 1 of n [`Layer`] allows for picking from several puzzles at runtime without revealing
/// them up front.
///
/// The puzzles are the leaves of a merkle tree. To spend the coin, one of them is revealed along
/// with a merkle proof that it's in the tree, and it's run with its solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2OneOfManyLayer {
    /// The merkle root of the tree hashes of the puzzles that can be used to spend the coin.
    pub merkle_root: Bytes32,
}

impl P2OneOfManyLayer {
    /// Creates a [`P2OneOfManyLayer`] from the merkle root of the puzzle hashes.
    pub fn new(merkle_root: Bytes32) -> Self {
        Self { merkle_root }
    }
}

impl Layer for P2OneOfManyLayer {
    type Solution = P2OneOfManySolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_1_OF_N_HASH.into() {
            return Ok(None);
        }

        let args = P2OneOfManyArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            merkle_root: args.merkle_root,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2OneOfManySolution::<NodePtr, NodePtr>::from_clvm(
            allocator, solution,
        )?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2OneOfManyArgs {
            merkle_root: self.merkle_root,
        })
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

impl ToTreeHash for P2OneOfManyLayer {
    fn tree_hash(&self) -> TreeHash {
        CurriedProgram {
            program: TreeHash::new(P2_1_OF_N_HASH),
            args: P2OneOfManyArgs::new(self.merkle_root),
        }
        .tree_hash()
    }
}
