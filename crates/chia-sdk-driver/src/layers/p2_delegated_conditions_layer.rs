use chia_bls::PublicKey;
use chia_sdk_types::Condition;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, TreeHash};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The p2 delegated conditions [`Layer`] allows a certain key to spend the coin.
/// To do so, a list of additional conditions is signed and passed in the solution.
/// Typically, the [`StandardLayer`](crate::StandardLayer) is used instead, since it adds more flexibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2DelegatedConditionsLayer {
    /// The public key that has the ability to spend the coin.
    pub public_key: PublicKey,
}

impl Layer for P2DelegatedConditionsLayer {
    type Solution = P2DelegatedConditionsSolution;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.p2_delegated_conditions_puzzle()?,
            args: P2DelegatedConditionsArgs::new(self.public_key),
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

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_DELEGATED_CONDITIONS_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2DelegatedConditionsArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            public_key: args.public_key,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2DelegatedConditionsSolution::from_clvm(
            allocator, solution,
        )?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2DelegatedConditionsArgs {
    pub public_key: PublicKey,
}

impl P2DelegatedConditionsArgs {
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2DelegatedConditionsSolution<T = NodePtr> {
    pub conditions: Vec<Condition<T>>,
}

impl P2DelegatedConditionsSolution {
    pub fn new(conditions: Vec<Condition>) -> Self {
        Self { conditions }
    }
}

pub const P2_DELEGATED_CONDITIONS_PUZZLE: [u8; 137] = hex!(
    "
    ff02ffff01ff04ffff04ff04ffff04ff05ffff04ffff02ff06ffff04ff02ffff
    04ff0bff80808080ff80808080ff0b80ffff04ffff01ff32ff02ffff03ffff07
    ff0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080
    ffff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff05
    8080ff0180ff018080
    "
);

pub const P2_DELEGATED_CONDITIONS_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "0ff94726f1a8dea5c3f70d3121945190778d3b2b3fcda3735a1f290977e98341"
));
