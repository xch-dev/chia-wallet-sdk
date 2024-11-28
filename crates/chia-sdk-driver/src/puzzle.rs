use clvm_traits::{FromClvm, FromClvmError};
use clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, SpendContext};

#[derive(Debug, Clone, Copy)]
pub enum Puzzle {
    Curried(CurriedPuzzle),
    Raw(RawPuzzle),
}

impl Puzzle {
    pub fn parse(allocator: &Allocator, puzzle: NodePtr) -> Self {
        CurriedPuzzle::parse(allocator, puzzle).map_or_else(
            || {
                Self::Raw(RawPuzzle {
                    puzzle_hash: tree_hash(allocator, puzzle),
                    ptr: puzzle,
                })
            },
            Self::Curried,
        )
    }

    pub fn curried_puzzle_hash(&self) -> TreeHash {
        match self {
            Self::Curried(curried) => curried.curried_puzzle_hash,
            Self::Raw(raw) => raw.puzzle_hash,
        }
    }

    pub fn mod_hash(&self) -> TreeHash {
        match self {
            Self::Curried(curried) => curried.mod_hash,
            Self::Raw(raw) => raw.puzzle_hash,
        }
    }

    pub fn ptr(&self) -> NodePtr {
        match self {
            Self::Curried(curried) => curried.curried_ptr,
            Self::Raw(raw) => raw.ptr,
        }
    }

    pub fn is_curried(&self) -> bool {
        matches!(self, Self::Curried(_))
    }

    pub fn is_raw(&self) -> bool {
        matches!(self, Self::Raw(_))
    }

    pub fn as_curried(&self) -> Option<CurriedPuzzle> {
        match self {
            Self::Curried(curried) => Some(*curried),
            Self::Raw(_raw) => None,
        }
    }

    pub fn as_raw(&self) -> Option<RawPuzzle> {
        match self {
            Self::Curried(_curried) => None,
            Self::Raw(raw) => Some(*raw),
        }
    }
}

impl PartialEq for Puzzle {
    fn eq(&self, other: &Self) -> bool {
        self.curried_puzzle_hash() == other.curried_puzzle_hash()
    }
}

impl Eq for Puzzle {}

impl FromClvm<Allocator> for Puzzle {
    fn from_clvm(allocator: &Allocator, puzzle: NodePtr) -> Result<Self, FromClvmError> {
        Ok(Self::parse(allocator, puzzle))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CurriedPuzzle {
    pub curried_puzzle_hash: TreeHash,
    pub curried_ptr: NodePtr,
    pub mod_hash: TreeHash,
    pub args: NodePtr,
}

impl CurriedPuzzle {
    pub fn parse(allocator: &Allocator, puzzle: NodePtr) -> Option<Self> {
        let curried = CurriedProgram::from_clvm(allocator, puzzle).ok()?;
        let mod_hash = tree_hash(allocator, curried.program);
        let curried_puzzle_hash = CurriedProgram {
            program: mod_hash,
            args: tree_hash(allocator, curried.args),
        }
        .tree_hash();

        Some(Self {
            curried_puzzle_hash,
            curried_ptr: puzzle,
            mod_hash,
            args: curried.args,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RawPuzzle {
    pub puzzle_hash: TreeHash,
    pub ptr: NodePtr,
}

impl ToTreeHash for Puzzle {
    fn tree_hash(&self) -> TreeHash {
        self.curried_puzzle_hash()
    }
}

impl Layer for Puzzle {
    type Solution = NodePtr;

    fn parse_puzzle(_allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        Ok(Some(puzzle))
    }

    fn parse_solution(
        _allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(solution)
    }

    fn construct_puzzle(&self, _ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        Ok(self.ptr())
    }

    fn construct_solution(
        &self,
        _ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        Ok(solution)
    }
}
