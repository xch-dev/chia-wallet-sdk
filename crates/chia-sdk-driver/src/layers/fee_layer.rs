use chia_protocol::Bytes32;
use chia_sdk_puzzles::FEE_LAYER_V1_HASH;
use chia_sdk_types::{
    Mod,
    puzzles::{FeeLayerArgs, FeeLayerSolution},
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The fee [`Layer`] enforces issuer-fee payment assertions on transfers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeLayer<I> {
    pub issuer_fee_puzzle_hash: Bytes32,
    pub fee_basis_points: u16,
    pub min_fee: u64,
    pub allow_zero_price: bool,
    pub allow_revoke_fee_bypass: bool,
    pub has_hidden_revoke_layer: bool,
    pub inner_puzzle: I,
}

impl<I> FeeLayer<I> {
    pub fn new(
        issuer_fee_puzzle_hash: Bytes32,
        fee_basis_points: u16,
        min_fee: u64,
        allow_zero_price: bool,
        allow_revoke_fee_bypass: bool,
        has_hidden_revoke_layer: bool,
        inner_puzzle: I,
    ) -> Self {
        Self {
            issuer_fee_puzzle_hash,
            fee_basis_points,
            min_fee,
            allow_zero_price,
            allow_revoke_fee_bypass,
            has_hidden_revoke_layer,
            inner_puzzle,
        }
    }
}

impl<I> Layer for FeeLayer<I>
where
    I: Layer,
{
    type Solution = FeeLayerSolution<I::Solution>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != FEE_LAYER_V1_HASH.into() {
            return Ok(None);
        }

        let args = FeeLayerArgs::<NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.mod_hash != FEE_LAYER_V1_HASH.into() {
            return Err(DriverError::InvalidModHash);
        }

        let Some(inner_puzzle) =
            I::parse_puzzle(allocator, Puzzle::parse(allocator, args.inner_puzzle))?
        else {
            return Ok(None);
        };

        Ok(Some(Self {
            issuer_fee_puzzle_hash: args.issuer_fee_puzzle_hash,
            fee_basis_points: args.fee_basis_points,
            min_fee: args.min_fee,
            allow_zero_price: args.allow_zero_price,
            allow_revoke_fee_bypass: args.allow_revoke_fee_bypass,
            has_hidden_revoke_layer: args.has_hidden_revoke_layer,
            inner_puzzle,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        let solution = FeeLayerSolution::<NodePtr>::from_clvm(allocator, solution)?;
        let inner_solution = I::parse_solution(allocator, solution.inner_solution)?;
        Ok(FeeLayerSolution::new(inner_solution))
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        ctx.curry(FeeLayerArgs::new(
            self.issuer_fee_puzzle_hash,
            self.fee_basis_points,
            self.min_fee,
            self.allow_zero_price,
            self.allow_revoke_fee_bypass,
            self.has_hidden_revoke_layer,
            inner_puzzle,
        ))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let inner_solution = self
            .inner_puzzle
            .construct_solution(ctx, solution.inner_solution)?;

        ctx.alloc(&FeeLayerSolution::new(inner_solution))
    }
}

impl<I> ToTreeHash for FeeLayer<I>
where
    I: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        FeeLayerArgs::new(
            self.issuer_fee_puzzle_hash,
            self.fee_basis_points,
            self.min_fee,
            self.allow_zero_price,
            self.allow_revoke_fee_bypass,
            self.has_hidden_revoke_layer,
            self.inner_puzzle.tree_hash(),
        )
        .curry_tree_hash()
    }
}
