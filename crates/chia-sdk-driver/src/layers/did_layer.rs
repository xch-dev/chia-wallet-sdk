use chia_protocol::Bytes32;
use chia_puzzles::{
    did::{DidArgs, DidSolution, DID_INNER_PUZZLE_HASH},
    singleton::SingletonStruct,
};
use clvm_traits::{ClvmEncoder, FromClvm, ToClvm, ToClvmError};
use clvm_utils::{CurriedProgram, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug)]
pub struct DidLayer<M, I> {
    pub launcher_id: Bytes32,
    pub recovery_list_hash: Bytes32,
    pub num_verifications_required: u64,
    pub metadata: M,
    pub inner_puzzle: I,
}

impl<M, I> DidLayer<M, I> {
    pub fn new(
        launcher_id: Bytes32,
        recovery_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
        inner_puzzle: I,
    ) -> Self {
        Self {
            launcher_id,
            recovery_list_hash,
            num_verifications_required,
            metadata,
            inner_puzzle,
        }
    }
}

impl<M, I> Layer for DidLayer<M, I>
where
    I: Layer,
    M: ToClvm<Allocator> + FromClvm<Allocator>,
{
    type Solution = DidSolution<I::Solution>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != DID_INNER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = DidArgs::<NodePtr, M>::from_clvm(allocator, puzzle.args)?;

        let Some(inner_puzzle) =
            I::parse_puzzle(allocator, Puzzle::parse(allocator, args.inner_puzzle))?
        else {
            return Ok(None);
        };

        Ok(Some(Self {
            launcher_id: args.singleton_struct.launcher_id,
            recovery_list_hash: args.recovery_list_hash,
            num_verifications_required: args.num_verifications_required,
            metadata: args.metadata,
            inner_puzzle,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        match DidSolution::<NodePtr>::from_clvm(allocator, solution)? {
            DidSolution::Spend(inner_solution) => {
                let inner_solution = I::parse_solution(allocator, inner_solution)?;
                Ok(DidSolution::Spend(inner_solution))
            }
            DidSolution::Recover(recovery) => Ok(DidSolution::Recover(recovery)),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.did_inner_puzzle().map_err(DriverError::Spend)?,
            args: DidArgs::new(
                self.inner_puzzle.construct_puzzle(ctx)?,
                self.recovery_list_hash,
                self.num_verifications_required,
                SingletonStruct::new(self.launcher_id),
                &self.metadata,
            ),
        };
        Ok(ctx.alloc(&curried)?)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        match solution {
            DidSolution::Spend(inner_solution) => {
                let inner_solution = self.inner_puzzle.construct_solution(ctx, inner_solution)?;
                Ok(ctx.alloc(&DidSolution::Spend(inner_solution))?)
            }
            DidSolution::Recover(recovery) => {
                Ok(ctx.alloc(&DidSolution::<NodePtr>::Recover(recovery))?)
            }
        }
    }
}

impl<E, M, I> ToClvm<E> for DidLayer<M, I>
where
    M: ToClvm<E>,
    I: ToClvm<E>,
    TreeHash: ToClvm<E>,
    E: ClvmEncoder<Node = TreeHash>,
{
    fn to_clvm(&self, encoder: &mut E) -> Result<TreeHash, ToClvmError> {
        CurriedProgram {
            program: DID_INNER_PUZZLE_HASH,
            args: DidArgs::new(
                &self.inner_puzzle,
                self.recovery_list_hash,
                self.num_verifications_required,
                SingletonStruct::new(self.launcher_id),
                &self.metadata,
            ),
        }
        .to_clvm(encoder)
    }
}
