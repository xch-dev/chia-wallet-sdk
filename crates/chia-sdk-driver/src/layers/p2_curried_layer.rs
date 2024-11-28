use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, TreeHash};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The p2 curried [`Layer`] allows for .
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2CurriedLayer {
    pub puzzle_hash: Bytes32,
}

impl Layer for P2CurriedLayer {
    type Solution = P2CurriedSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_CURRIED_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2CurriedArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            puzzle_hash: args.puzzle_hash,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2CurriedSolution::<NodePtr, NodePtr>::from_clvm(
            allocator, solution,
        )?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.p2_curried_puzzle()?,
            args: P2CurriedArgs {
                puzzle_hash: self.puzzle_hash,
            },
        };
        ctx.alloc(&curried)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2CurriedArgs {
    pub puzzle_hash: Bytes32,
}

impl P2CurriedArgs {
    pub fn new(puzzle_hash: Bytes32) -> Self {
        Self { puzzle_hash }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2CurriedSolution<P, S> {
    pub puzzle: P,
    pub solution: S,
}

impl<P, S> P2CurriedSolution<P, S> {
    pub fn new(puzzle: P, solution: S) -> Self {
        Self { puzzle, solution }
    }
}

pub const P2_CURRIED_PUZZLE: [u8; 143] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ff05ffff02ff02ffff04ff02ffff04ff0bff80
    80808080ffff01ff02ff0bff1780ffff01ff088080ff0180ffff04ffff01ff02
    ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff02ffff04ff02ffff04ff
    09ff80808080ffff02ff02ffff04ff02ffff04ff0dff8080808080ffff01ff0b
    ffff0101ff058080ff0180ff018080
    "
);

pub const P2_CURRIED_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "13e29a62b42cd2ef72a79e4bacdc59733ca6310d65af83d349360d36ec622363"
));

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(P2_CURRIED_PUZZLE => P2_CURRIED_PUZZLE_HASH);
        Ok(())
    }
}
