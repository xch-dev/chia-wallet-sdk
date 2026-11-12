use crate::{DriverError, Layer, Puzzle, SpendContext};
use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod,
    puzzles::{
        InternalCustodyPrefarmArgs, InternalCustodyPuzzleHashes, InternalCustodySpendSolution,
        InternalCustodyState, InternalCustodyTimelocks,
    },
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
    /// Rekey branch puzzle commitment.
    pub rekey_puzzle_hash: Bytes32,
    /// ACH branch puzzle commitment.
    pub payment_puzzle_hash: Bytes32,
    /// Minimum singleton age in seconds before a withdrawal or absorption.
    pub withdrawal_timelock: u64,
    /// Base rekey timelock in seconds.
    pub rekey_increment: u64,
    /// Additional penalty in seconds for slower rekey paths.
    pub slow_rekey: u64,
}
impl InternalCustodyLayer {
    /// Return the typed curry arguments for this layer.
    pub fn args(&self) -> InternalCustodyPrefarmArgs {
        InternalCustodyPrefarmArgs {
            mod_hash: InternalCustodyPrefarmArgs::mod_hash().into(),
            puzzle_root: self.puzzle_root,
            config: InternalCustodyState {
                puzzles: InternalCustodyPuzzleHashes {
                    rekey: self.rekey_puzzle_hash,
                    payment: self.payment_puzzle_hash,
                },
                timelocks: InternalCustodyTimelocks {
                    withdrawal: self.withdrawal_timelock,
                    rekey_increment: self.rekey_increment,
                    slow_rekey: self.slow_rekey,
                },
            },
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
            rekey_puzzle_hash: args.config.puzzles.rekey,
            payment_puzzle_hash: args.config.puzzles.payment,
            withdrawal_timelock: args.config.timelocks.withdrawal,
            rekey_increment: args.config.timelocks.rekey_increment,
            slow_rekey: args.config.timelocks.slow_rekey,
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
