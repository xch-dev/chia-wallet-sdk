use chia_protocol::Bytes32;
use chia_puzzles::{
    did::{DidArgs, DidSolution, DID_INNER_PUZZLE_HASH},
    singleton::SingletonStruct,
};
use clvm_traits::{FromClvm, ToClvm, ToNodePtr};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Puzzle, PuzzleLayer, SpendContext};

#[derive(Debug)]
pub struct DIDLayer<M, IP> {
    pub launcher_id: Bytes32,
    pub recovery_did_list_hash: Bytes32,
    pub num_verifications_required: u64,
    pub metadata: M,
    pub inner_puzzle: IP,
}

#[derive(Debug, ToClvm, FromClvm)]
#[clvm(list)]
pub struct DIDLayerSolution<I> {
    pub inner_solution: I,
}

impl<M, IP> PuzzleLayer for DIDLayer<M, IP>
where
    IP: PuzzleLayer,
    M: FromClvm<NodePtr> + ToClvm<NodePtr>,
{
    type Solution = DIDLayerSolution<IP::Solution>;

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
            .map_err(|err| DriverError::FromClvm(err))?;

        let parent_inner_sol = match DidSolution::<NodePtr>::from_clvm(allocator, layer_solution)
            .map_err(|err| DriverError::FromClvm(err))?
        {
            DidSolution::InnerSpend(inner_solution) => inner_solution,
        };

        match IP::from_parent_spend(allocator, parent_args.inner_puzzle, parent_inner_sol)? {
            None => return Ok(None),
            Some(inner_puzzle) => Ok(Some(DIDLayer::<M, IP> {
                launcher_id: parent_args.singleton_struct.launcher_id,
                recovery_did_list_hash: parent_args.recovery_did_list_hash,
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
            .map_err(|err| DriverError::FromClvm(err))?;

        match IP::from_puzzle(allocator, args.inner_puzzle)? {
            None => return Ok(None),
            Some(inner_puzzle) => Ok(Some(DIDLayer::<M, IP> {
                launcher_id: args.singleton_struct.launcher_id,
                recovery_did_list_hash: args.recovery_did_list_hash,
                num_verifications_required: args.num_verifications_required,
                metadata: args.metadata,
                inner_puzzle,
            })),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let metadata_ptr = self
            .metadata
            .to_node_ptr(ctx.allocator_mut())
            .map_err(|err| DriverError::ToClvm(err))?;

        CurriedProgram {
            program: ctx
                .did_inner_puzzle()
                .map_err(|err| DriverError::Spend(err))?,
            args: DidArgs {
                recovery_did_list_hash: self.recovery_did_list_hash,
                num_verifications_required: self.num_verifications_required,
                singleton_struct: SingletonStruct::new(self.launcher_id),
                metadata: metadata_ptr,
                inner_puzzle: self.inner_puzzle.construct_puzzle(ctx)?,
            },
        }
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| DriverError::ToClvm(err))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        DidSolution::InnerSpend(
            self.inner_puzzle
                .construct_solution(ctx, solution.inner_solution)?,
        )
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| DriverError::ToClvm(err))
    }
}

impl<M, IP> ToTreeHash for DIDLayer<M, IP>
where
    IP: ToTreeHash,
    M: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        CurriedProgram {
            program: DID_INNER_PUZZLE_HASH,
            args: DidArgs {
                recovery_did_list_hash: self.recovery_did_list_hash,
                num_verifications_required: self.num_verifications_required,
                singleton_struct: SingletonStruct::new(self.launcher_id),
                metadata: self.metadata.tree_hash(),
                inner_puzzle: self.inner_puzzle.tree_hash(),
            },
        }
        .tree_hash()
    }
}
