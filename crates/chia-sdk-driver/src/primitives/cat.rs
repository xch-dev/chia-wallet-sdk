use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    cat::{CatArgs, CatSolution},
    CoinProof, LineageProof,
};
use chia_sdk_types::{run_puzzle, Condition};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{CatLayer, DriverError, Layer, Primitive, Puzzle, Spend, SpendContext};

#[derive(Debug, Clone, Copy)]
pub struct Cat {
    pub coin: Coin,
    pub lineage_proof: Option<LineageProof>,
    pub asset_id: Bytes32,
    pub p2_puzzle_hash: Bytes32,
}

impl Cat {
    pub fn new(
        coin: Coin,
        lineage_proof: Option<LineageProof>,
        asset_id: Bytes32,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            coin,
            lineage_proof,
            asset_id,
            p2_puzzle_hash,
        }
    }

    /// Creates a coin spend for this CAT.
    #[must_use]
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        prev_coin_id: Bytes32,
        this_coin_info: Coin,
        next_coin_proof: CoinProof,
        prev_subtotal: i64,
        extra_delta: i64,
        inner_spend: Spend,
    ) -> Result<CoinSpend, DriverError> {
        let cat_layer = CatLayer::new(self.asset_id, inner_spend.puzzle);

        let puzzle_ptr = cat_layer.construct_puzzle(ctx)?;
        let solution_ptr = cat_layer.construct_solution(
            ctx,
            CatSolution {
                lineage_proof: self.lineage_proof,
                prev_coin_id,
                this_coin_info,
                next_coin_proof,
                prev_subtotal,
                extra_delta,
                inner_puzzle_solution: inner_spend.solution,
            },
        )?;

        let puzzle = ctx.serialize(&puzzle_ptr)?;
        let solution = ctx.serialize(&solution_ptr)?;

        Ok(CoinSpend::new(self.coin, puzzle, solution))
    }

    /// Returns the lineage proof that would be used by each child.
    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.p2_puzzle_hash.into(),
            parent_amount: self.coin.amount,
        }
    }

    /// Creates a wrapped spendable CAT for a given output.
    pub fn wrapped_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        let puzzle_hash = CatArgs::curry_tree_hash(self.asset_id, p2_puzzle_hash.into());
        Self {
            coin: Coin::new(self.coin.coin_id(), puzzle_hash.into(), amount),
            lineage_proof: Some(self.child_lineage_proof()),
            asset_id: self.asset_id,
            p2_puzzle_hash,
        }
    }
}

impl Primitive for Cat {
    fn from_parent_spend(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
        coin: Coin,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(parent_layer) = CatLayer::<Puzzle>::parse_puzzle(allocator, parent_puzzle)? else {
            return Ok(None);
        };
        let parent_solution = CatLayer::<Puzzle>::parse_solution(allocator, parent_solution)?;

        let output = run_puzzle(
            allocator,
            parent_layer.inner_puzzle.ptr(),
            parent_solution.inner_puzzle_solution,
        )?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let p2_puzzle_hash = conditions
            .into_iter()
            .filter_map(Condition::into_create_coin)
            .find_map(|create_coin| {
                // This is an optimization to skip calculating the hash.
                if create_coin.amount != coin.amount {
                    return None;
                }

                // Calculate what the wrapped puzzle hash would be for the created coin.
                // This is because we're running the inner layer.
                let wrapped_puzzle_hash =
                    CatArgs::curry_tree_hash(parent_layer.asset_id, create_coin.puzzle_hash.into());

                // If the puzzle hash doesn't match the coin, this isn't the correct p2 puzzle hash.
                if wrapped_puzzle_hash != coin.puzzle_hash.into() {
                    return None;
                }

                // We've found the p2 puzzle hash of the coin we're looking for.
                Some(create_coin.puzzle_hash)
            });

        let Some(p2_puzzle_hash) = p2_puzzle_hash else {
            return Err(DriverError::MissingChild);
        };

        Ok(Some(Self {
            coin,
            lineage_proof: Some(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash: parent_layer.inner_puzzle.curried_puzzle_hash().into(),
                parent_amount: parent_coin.amount,
            }),
            asset_id: parent_layer.asset_id,
            p2_puzzle_hash,
        }))
    }
}
