use chia_protocol::Bytes32;
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_MEMBER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_sdk_types::puzzles::{SingletonMember, SingletonMemberSolution};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SingletonMemberLayer {
    pub launcher_id: Bytes32,
}

impl SingletonMemberLayer {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self { launcher_id }
    }
}

impl Layer for SingletonMemberLayer {
    type Solution = SingletonMemberSolution;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(SingletonMember::new(self.launcher_id))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(curried) = puzzle.as_curried() else {
            return Ok(None);
        };

        if curried.mod_hash != SINGLETON_MEMBER_HASH.into() {
            return Ok(None);
        }

        let args = SingletonMember::from_clvm(allocator, curried.args)?;

        if args.singleton_struct.mod_hash != SINGLETON_TOP_LAYER_V1_1_HASH.into()
            || args.singleton_struct.launcher_puzzle_hash != SINGLETON_LAUNCHER_HASH.into()
        {
            return Err(DriverError::InvalidSingletonStruct);
        }

        Ok(Some(Self::new(args.singleton_struct.launcher_id)))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(SingletonMemberSolution::from_clvm(allocator, solution)?)
    }
}
