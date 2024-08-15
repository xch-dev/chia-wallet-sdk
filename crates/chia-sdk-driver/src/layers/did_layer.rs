use chia_protocol::Bytes32;
use chia_puzzles::{
    did::{DidArgs, DidSolution, DID_INNER_PUZZLE_HASH},
    singleton::SingletonStruct,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug)]
pub struct DidLayer<M, IP> {
    pub launcher_id: Bytes32,
    pub recovery_list_hash: Bytes32,
    pub num_verifications_required: u64,
    pub metadata: M,
    pub inner_puzzle: IP,
}

#[derive(Debug, ToClvm, FromClvm)]
#[clvm(list)]
pub struct DidLayerSolution<I> {
    pub inner_solution: I,
}

impl<M, IP> Layer for DidLayer<M, IP>
where
    IP: Layer,
    M: FromClvm<Allocator> + ToClvm<Allocator>,
{
    type Solution = DidLayerSolution<IP::Solution>;

    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let parent_puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(parent_puzzle) = parent_puzzle.as_curried() else {
            return Ok(None);
        };

        if parent_puzzle.mod_hash != DID_INNER_PUZZLE_HASH {
            return Ok(None);
        }

        let parent_args = DidArgs::<NodePtr, M>::from_clvm(allocator, parent_puzzle.args)
            .map_err(DriverError::FromClvm)?;

        let parent_inner_solution =
            match DidSolution::<NodePtr>::from_clvm(allocator, layer_solution)
                .map_err(DriverError::FromClvm)?
            {
                DidSolution::Spend(inner_solution) => inner_solution,
            };

        match IP::from_parent_spend(allocator, parent_args.inner_puzzle, parent_inner_sol)? {
            None => Ok(None),
            Some(inner_puzzle) => Ok(Some(DidLayer::<M, IP> {
                launcher_id: parent_args.singleton_struct.launcher_id,
                recovery_list_hash: parent_args.recovery_list_hash,
                num_verifications_required: parent_args.num_verifications_required,
                metadata: parent_args.metadata,
                inner_puzzle,
            })),
        }
    }

    fn from_puzzle(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != DID_INNER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = DidArgs::<NodePtr, M>::from_clvm(allocator, puzzle.args)
            .map_err(DriverError::FromClvm)?;

        match IP::from_puzzle(allocator, args.inner_puzzle)? {
            None => Ok(None),
            Some(inner_puzzle) => Ok(Some(DidLayer::<M, IP> {
                launcher_id: args.singleton_struct.launcher_id,
                recovery_list_hash: args.recovery_list_hash,
                num_verifications_required: args.num_verifications_required,
                metadata: args.metadata,
                inner_puzzle,
            })),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let metadata_ptr = self
            .metadata
            .to_clvm(ctx.allocator_mut())
            .map_err(DriverError::ToClvm)?;

        CurriedProgram {
            program: ctx.did_inner_puzzle().map_err(DriverError::Spend)?,
            args: DidArgs {
                recovery_list_hash: self.recovery_list_hash,
                num_verifications_required: self.num_verifications_required,
                singleton_struct: SingletonStruct::new(self.launcher_id),
                metadata: metadata_ptr,
                inner_puzzle: self.inner_puzzle.construct_puzzle(ctx)?,
            },
        }
        .to_clvm(ctx.allocator_mut())
        .map_err(DriverError::ToClvm)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        DidSolution::Spend(
            self.inner_puzzle
                .construct_solution(ctx, solution.inner_solution)?,
        )
        .to_clvm(ctx.allocator_mut())
        .map_err(DriverError::ToClvm)
    }
}

impl<M, IP> ToTreeHash for DidLayer<M, IP>
where
    IP: ToTreeHash,
    M: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        CurriedProgram {
            program: DID_INNER_PUZZLE_HASH,
            args: DidArgs {
                recovery_list_hash: self.recovery_list_hash,
                num_verifications_required: self.num_verifications_required,
                singleton_struct: SingletonStruct::new(self.launcher_id),
                metadata: self.metadata.tree_hash(),
                inner_puzzle: self.inner_puzzle.tree_hash(),
            },
        }
        .tree_hash()
    }
}

impl<M, IP> DidLayer<M, IP> {
    pub fn wrap_inner_puzzle_hash(
        launcher_id: Bytes32,
        recovery_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata_hash: TreeHash,
        inner_puzzle_hash: TreeHash,
    ) -> TreeHash {
        CurriedProgram {
            program: DID_INNER_PUZZLE_HASH,
            args: DidArgs {
                recovery_list_hash,
                num_verifications_required,
                singleton_struct: SingletonStruct::new(launcher_id),
                metadata: metadata_hash,
                inner_puzzle: inner_puzzle_hash,
            },
        }
        .tree_hash()
    }
}
