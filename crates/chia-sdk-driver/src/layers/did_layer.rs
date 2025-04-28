use chia_protocol::Bytes32;
use chia_puzzle_types::{
    did::{DidArgs, DidSolution},
    singleton::SingletonStruct,
};
use chia_puzzles::{DID_INNERPUZ_HASH, SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The DID [`Layer`] keeps track of metadata and handles recovery capabilities.
/// It's typically an inner layer of the [`SingletonLayer`](crate::SingletonLayer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DidLayer<M, I> {
    /// The unique launcher id for the DID. Also referred to as the DID id.
    pub launcher_id: Bytes32,
    /// The tree hash of an optional list of recovery DIDs.
    pub recovery_list_hash: Option<Bytes32>,
    /// The number of verifications required to recover the DID.
    pub num_verifications_required: u64,
    /// Metadata associated with the DID. This is often just `()` for DIDs without metadata.
    pub metadata: M,
    /// The inner puzzle layer, commonly used for determining ownership.
    pub inner_puzzle: I,
}

impl<M, I> DidLayer<M, I> {
    pub fn new(
        launcher_id: Bytes32,
        recovery_list_hash: Option<Bytes32>,
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

    pub fn with_metadata<N>(self, metadata: N) -> DidLayer<N, I> {
        DidLayer {
            launcher_id: self.launcher_id,
            recovery_list_hash: self.recovery_list_hash,
            num_verifications_required: self.num_verifications_required,
            metadata,
            inner_puzzle: self.inner_puzzle,
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

        if puzzle.mod_hash != DID_INNERPUZ_HASH.into() {
            return Ok(None);
        }

        let args = DidArgs::<NodePtr, M>::from_clvm(allocator, puzzle.args)?;

        if args.singleton_struct.mod_hash != SINGLETON_TOP_LAYER_V1_1_HASH.into()
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
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        ctx.curry(DidArgs::new(
            inner_puzzle,
            self.recovery_list_hash,
            self.num_verifications_required,
            SingletonStruct::new(self.launcher_id),
            &self.metadata,
        ))
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

impl<M, I> ToTreeHash for DidLayer<M, I>
where
    M: ToTreeHash,
    I: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        let inner_puzzle_hash = self.inner_puzzle.tree_hash();
        let metadata_hash = self.metadata.tree_hash();
        DidArgs::curry_tree_hash(
            inner_puzzle_hash,
            self.recovery_list_hash,
            self.num_verifications_required,
            SingletonStruct::new(self.launcher_id),
            metadata_hash,
        )
    }
}
