use crate::{DriverError, Layer, Puzzle, SpendContext};
use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod,
    puzzles::{InternalCustodyPrefarmArgs, InternalCustodySpendSolution, InternalCustodyState},
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

/// The inner legacy prefarm layer. Parsing this layer identifies its parameters;
/// it does not authorize a spend or establish a trusted policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternalCustodyLayer {
    /// Merkle root committing to the authorized custody puzzles.
    pub puzzle_root: Bytes32,
    /// Immutable puzzle commitments and timing configuration.
    pub state: InternalCustodyState,
}
impl InternalCustodyLayer {
    /// Return the typed curry arguments for this layer.
    pub fn args(&self) -> InternalCustodyPrefarmArgs {
        InternalCustodyPrefarmArgs {
            mod_hash: InternalCustodyPrefarmArgs::mod_hash().into(),
            puzzle_root: self.puzzle_root,
            config: self.state,
        }
    }
}
impl Layer for InternalCustodyLayer {
    type Solution = InternalCustodySpendSolution<NodePtr, NodePtr, NodePtr, NodePtr>;
    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };
        if puzzle.mod_hash != InternalCustodyPrefarmArgs::mod_hash() {
            return Ok(None);
        }
        let args = InternalCustodyPrefarmArgs::from_clvm(allocator, puzzle.args)?;
        if args.mod_hash != InternalCustodyPrefarmArgs::mod_hash().into() {
            return Err(DriverError::InvalidModHash);
        }
        Ok(Some(Self {
            puzzle_root: args.puzzle_root,
            state: args.config,
        }))
    }
    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(Self::Solution::from_clvm(allocator, solution)?)
    }
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(self.args())
    }
    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}
impl ToTreeHash for InternalCustodyLayer {
    fn tree_hash(&self) -> TreeHash {
        self.args().curry_tree_hash()
    }
}
