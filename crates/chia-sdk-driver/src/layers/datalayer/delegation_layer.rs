use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::{
    DelegationLayerArgs, DelegationLayerSolution, DELEGATION_LAYER_PUZZLE_HASH,
};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[allow(clippy::doc_markdown)]
/// The Delegation [`Layer`] is used to enable DataLayer delegation capabilities
/// For more information, see CHIP-0035.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelegationLayer {
    /// Launcher ID of the singleton outer layer. Used as the default hint when recreating this layer.
    pub launcher_id: Bytes32,
    /// Puzzle hash of the owner (usually a p2 puzzle like the standard puzzle).
    pub owner_puzzle_hash: Bytes32,
    /// Merkle root corresponding to the tree of delegated puzzles.
    pub merkle_root: Bytes32,
}

impl DelegationLayer {
    pub fn new(launcher_id: Bytes32, owner_puzzle_hash: Bytes32, merkle_root: Bytes32) -> Self {
        Self {
            launcher_id,
            owner_puzzle_hash,
            merkle_root,
        }
    }
}

impl Layer for DelegationLayer {
    type Solution = DelegationLayerSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != DELEGATION_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = DelegationLayerArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            launcher_id: args.launcher_id,
            owner_puzzle_hash: args.owner_puzzle_hash,
            merkle_root: args.merkle_root,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(DelegationLayerSolution::<NodePtr, NodePtr>::from_clvm(
            allocator, solution,
        )?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = ctx.curry(DelegationLayerArgs::new(
            self.launcher_id,
            self.owner_puzzle_hash,
            self.merkle_root,
        ))?;
        ctx.alloc(&curried)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}
