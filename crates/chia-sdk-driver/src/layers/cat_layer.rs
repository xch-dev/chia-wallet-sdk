use chia_protocol::Bytes32;
use chia_puzzle_types::cat::{CatArgs, CatSolution};
use chia_puzzles::CAT_PUZZLE_HASH;
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The CAT [`Layer`] enforces restrictions on the supply of a token.
/// Specifically, unless the TAIL program is run, the supply cannot change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatLayer<I> {
    /// The asset id of the CAT token. This is the tree hash of the TAIL program.
    pub asset_id: Bytes32,
    /// The inner puzzle layer, commonly used for determining ownership.
    pub inner_puzzle: I,
}

impl<I> CatLayer<I> {
    pub fn new(asset_id: Bytes32, inner_puzzle: I) -> Self {
        Self {
            asset_id,
            inner_puzzle,
        }
    }
}

impl<I> Layer for CatLayer<I>
where
    I: Layer,
{
    type Solution = CatSolution<I::Solution>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != CAT_PUZZLE_HASH.into() {
            return Ok(None);
        }

        let args = CatArgs::<NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.mod_hash != CAT_PUZZLE_HASH.into() {
            return Err(DriverError::InvalidModHash);
        }

        let Some(inner_puzzle) =
            I::parse_puzzle(allocator, Puzzle::parse(allocator, args.inner_puzzle))?
        else {
            return Ok(None);
        };

        Ok(Some(Self {
            asset_id: args.asset_id,
            inner_puzzle,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        let solution = CatSolution::<NodePtr>::from_clvm(allocator, solution)?;
        let inner_solution = I::parse_solution(allocator, solution.inner_puzzle_solution)?;
        Ok(CatSolution {
            inner_puzzle_solution: inner_solution,
            lineage_proof: solution.lineage_proof,
            prev_coin_id: solution.prev_coin_id,
            this_coin_info: solution.this_coin_info,
            next_coin_proof: solution.next_coin_proof,
            prev_subtotal: solution.prev_subtotal,
            extra_delta: solution.extra_delta,
        })
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let inner_puzzle = self.inner_puzzle.construct_puzzle(ctx)?;
        ctx.curry(CatArgs::new(self.asset_id, inner_puzzle))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let inner_solution = self
            .inner_puzzle
            .construct_solution(ctx, solution.inner_puzzle_solution)?;
        ctx.alloc(&CatSolution {
            inner_puzzle_solution: inner_solution,
            lineage_proof: solution.lineage_proof,
            prev_coin_id: solution.prev_coin_id,
            this_coin_info: solution.this_coin_info,
            next_coin_proof: solution.next_coin_proof,
            prev_subtotal: solution.prev_subtotal,
            extra_delta: solution.extra_delta,
        })
    }
}

impl<I> ToTreeHash for CatLayer<I>
where
    I: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        let inner_puzzle_hash = self.inner_puzzle.tree_hash();
        CatArgs::curry_tree_hash(self.asset_id, inner_puzzle_hash)
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::Coin;
    use chia_puzzle_types::CoinProof;

    use super::*;

    #[test]
    fn test_cat_layer() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();
        let asset_id = Bytes32::new([1; 32]);

        let layer = CatLayer::new(asset_id, "Hello, world!".to_string());

        let ptr = layer.construct_puzzle(&mut ctx)?;
        let puzzle = Puzzle::parse(&ctx, ptr);
        let roundtrip = CatLayer::<String>::parse_puzzle(&ctx, puzzle)?.expect("invalid CAT layer");

        assert_eq!(roundtrip.asset_id, layer.asset_id);
        assert_eq!(roundtrip.inner_puzzle, layer.inner_puzzle);

        let expected = CatArgs::curry_tree_hash(asset_id, layer.inner_puzzle.tree_hash());
        assert_eq!(hex::encode(ctx.tree_hash(ptr)), hex::encode(expected));

        Ok(())
    }

    #[test]
    fn test_cat_solution() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();

        let layer = CatLayer::new(Bytes32::default(), NodePtr::NIL);

        let solution = CatSolution {
            inner_puzzle_solution: NodePtr::NIL,
            lineage_proof: None,
            prev_coin_id: Bytes32::default(),
            this_coin_info: Coin::new(Bytes32::default(), Bytes32::default(), 42),
            next_coin_proof: CoinProof {
                parent_coin_info: Bytes32::default(),
                inner_puzzle_hash: Bytes32::default(),
                amount: 34,
            },
            prev_subtotal: 0,
            extra_delta: 0,
        };
        let expected_ptr = ctx.alloc(&solution)?;
        let expected_hash = ctx.tree_hash(expected_ptr);

        let actual_ptr = layer.construct_solution(&mut ctx, solution)?;
        let actual_hash = ctx.tree_hash(actual_ptr);

        assert_eq!(hex::encode(actual_hash), hex::encode(expected_hash));

        let roundtrip = CatLayer::<NodePtr>::parse_solution(&ctx, actual_ptr)?;
        assert_eq!(roundtrip, solution);

        Ok(())
    }
}
