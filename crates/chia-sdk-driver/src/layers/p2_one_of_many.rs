use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, TreeHash};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The p2 1 of n [`Layer`] allows for picking from several delegated puzzles at runtime without revealing up front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2OneOfMany {
    /// The merkle root used to lookup the delegated puzzle as part of the solution.
    pub merkle_root: Bytes32,
}

impl Layer for P2OneOfMany {
    type Solution = P2OneOfManySolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_ONE_OF_MANY_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2OneOfManyArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            merkle_root: args.merkle_root,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2OneOfManySolution::<NodePtr, NodePtr>::from_clvm(
            allocator, solution,
        )?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.p2_one_of_many_puzzle()?,
            args: P2OneOfManyArgs {
                merkle_root: self.merkle_root,
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
pub struct P2OneOfManyArgs {
    pub merkle_root: Bytes32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2OneOfManySolution<P, S> {
    pub merkle_proof: Bytes32,
    pub puzzle: P,
    pub solution: S,
}

pub const P2_ONE_OF_MANY_PUZZLE: [u8; 280] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ff05ffff02ff06ffff04ff02ffff04ffff0bff
    ff0101ffff02ff04ffff04ff02ffff04ff17ff8080808080ffff04ff0bff8080
    80808080ffff01ff02ff17ff2f80ffff01ff088080ff0180ffff04ffff01ffff
    02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff04ffff04ff02ffff04
    ff09ff80808080ffff02ff04ffff04ff02ffff04ff0dff8080808080ffff01ff
    0bffff0101ff058080ff0180ff02ffff03ff1bffff01ff02ff06ffff04ff02ff
    ff04ffff02ffff03ffff18ffff0101ff1380ffff01ff0bffff0102ff2bff0580
    ffff01ff0bffff0102ff05ff2b8080ff0180ffff04ffff04ffff17ff13ffff01
    81ff80ff3b80ff8080808080ffff010580ff0180ff018080
    "
);

pub const P2_ONE_OF_MANY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "46b29fd87fbeb6737600c4543931222a6c1ed3db6fa5601a3ca284a9f4efe780"
));

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(P2_ONE_OF_MANY_PUZZLE => P2_ONE_OF_MANY_PUZZLE_HASH);
        Ok(())
    }
}
