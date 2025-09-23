use chia_sdk_types::puzzles::{IndexWrapperArgs, INDEX_WRAPPER_HASH};
use clvm_traits::{FromClvm, MatchByte};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

pub type BulletinNonce = (MatchByte<98>, ());

/// The Bulletin [`Layer`] is used to wrap a puzzle hash with a static nonce
/// to differentiate it from normal XCH coins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BulletinLayer<I> {
    /// The inner puzzle layer, used to identify and spend the bulletin.
    pub inner_puzzle: I,
}

impl<I> BulletinLayer<I> {
    pub fn new(inner_puzzle: I) -> Self {
        Self { inner_puzzle }
    }
}

impl<I> Layer for BulletinLayer<I>
where
    I: Layer,
{
    type Solution = I::Solution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != INDEX_WRAPPER_HASH {
            return Ok(None);
        }

        let args = IndexWrapperArgs::<NodePtr, NodePtr>::from_clvm(allocator, puzzle.args)?;

        if BulletinNonce::from_clvm(allocator, args.nonce).is_err() {
            return Err(DriverError::InvalidModHash);
        }

        let Some(inner_puzzle) =
            I::parse_puzzle(allocator, Puzzle::parse(allocator, args.inner_puzzle))?
        else {
            return Ok(None);
        };

        Ok(Some(Self { inner_puzzle }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        I::parse_solution(allocator, solution)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        ctx.curry(IndexWrapperArgs::<BulletinNonce, _>::new(
            (MatchByte, ()),
            inner_puzzle,
        ))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        self.inner_puzzle.construct_solution(ctx, solution)
    }
}

impl<I> ToTreeHash for BulletinLayer<I>
where
    I: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        let inner_puzzle_hash = self.inner_puzzle.tree_hash();
        IndexWrapperArgs::<BulletinNonce, _>::new((MatchByte, ()), inner_puzzle_hash).tree_hash()
    }
}
