use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The Writer [`Layer`] removes an authorized puzzle's ability to change the list of authorized puzzles.
/// It's typically used with [`DelegationLayer`](crate::DelegationLayer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterLayer<I> {
    /// The inner puzzle layer, commonly used for determining ownership.
    pub inner_puzzle: I,
}

impl<I> WriterLayer<I> {
    pub fn new(inner_puzzle: I) -> Self {
        Self { inner_puzzle }
    }
}

impl<I> Layer for WriterLayer<I>
where
    I: Layer,
{
    type Solution = I::Solution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != WRITER_FILTER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = WriterLayerArgs::<NodePtr>::from_clvm(allocator, puzzle.args)?;

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
        let inner_solution =
            WriterLayerSolution::<NodePtr>::from_clvm(allocator, solution)?.inner_solution;

        I::parse_solution(allocator, inner_solution)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.did_inner_puzzle()?,
            args: WriterLayerArgs::new(self.inner_puzzle.construct_puzzle(ctx)?),
        };
        ctx.alloc(&curried)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let inner_solution = self.inner_puzzle.construct_solution(ctx, solution)?;

        ctx.alloc(&WriterLayerSolution::<NodePtr> { inner_solution })
    }
}

impl<I> ToTreeHash for WriterLayer<I>
where
    I: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        let inner_puzzle_hash = self.inner_puzzle.tree_hash();

        WriterLayerArgs::curry_tree_hash(inner_puzzle_hash)
    }
}

pub const WRITER_FILTER_PUZZLE: [u8; 110] = hex!(
    "
    ff02ffff01ff02ff02ffff04ff02ffff04ffff02ff05ff0b80ff80808080ffff04ffff01ff02ffff
    03ff05ffff01ff02ffff03ffff09ff11ffff0181f380ffff01ff0880ffff01ff04ff09ffff02ff02
    ffff04ff02ffff04ff0dff808080808080ff0180ff8080ff0180ff018080
    "
);

pub const WRITER_FILTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    407f70ea751c25052708219ae148b45db2f61af2287da53d600b2486f12b3ca6
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct WriterLayerArgs<I> {
    pub inner_puzzle: I,
}

impl<I> WriterLayerArgs<I> {
    pub fn new(inner_puzzle: I) -> Self {
        Self { inner_puzzle }
    }
}

impl WriterLayerArgs<TreeHash> {
    pub fn curry_tree_hash(inner_puzzle: TreeHash) -> TreeHash {
        CurriedProgram {
            program: WRITER_FILTER_PUZZLE_HASH,
            args: WriterLayerArgs { inner_puzzle },
        }
        .tree_hash()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct WriterLayerSolution<I> {
    pub inner_solution: I,
}
