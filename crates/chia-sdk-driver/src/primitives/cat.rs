use chia_protocol::{Bytes32, Coin};
use chia_puzzles::cat::CatArgs;
use chia_sdk_types::conditions::{run_puzzle, Condition};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{CatLayer, DriverError, Layer, Primitive, Puzzle};

#[derive(Debug, Clone, Copy)]
pub struct Cat {
    pub coin: Coin,
    pub asset_id: Bytes32,
    pub p2_puzzle_hash: Bytes32,
}

impl Cat {
    pub fn new(coin: Coin, asset_id: Bytes32, p2_puzzle_hash: Bytes32) -> Self {
        Self {
            coin,
            asset_id,
            p2_puzzle_hash,
        }
    }
}

impl Primitive for Cat {
    fn from_parent_spend(
        allocator: &mut Allocator,
        _parent_coin: Coin,
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
            asset_id: parent_layer.asset_id,
            p2_puzzle_hash,
        }))
    }
}

/*
#[allow(clippy::too_many_arguments)]
pub fn spend(
    &self,
    ctx: &mut SpendContext,
    lineage_proof: Option<LineageProof>,
    prev_coin_id: Bytes32,
    this_coin_info: Coin,
    next_coin_proof: CoinProof,
    prev_subtotal: i64,
    extra_delta: i64,
    inner_spend: Spend,
) -> Result<(CoinSpend, Cat, Proof), DriverError> {
    let thing = self.get_layered_object(Some(inner_spend.puzzle()));

    let puzzle_ptr = thing.construct_puzzle(ctx)?;
    let puzzle =
        Program::from_clvm(ctx.allocator(), puzzle_ptr).map_err(DriverError::FromClvm)?;

    let solution_ptr = thing.construct_solution(
        ctx,
        CatSolution {
            lineage_proof,
            prev_coin_id,
            this_coin_info,
            next_coin_proof,
            prev_subtotal,
            extra_delta,
            inner_puzzle_solution: inner_spend.solution(),
        },
    )?;
    let solution =
        Program::from_clvm(ctx.allocator(), solution_ptr).map_err(DriverError::FromClvm)?;

    let cs = CoinSpend {
        coin: self.coin,
        puzzle_reveal: puzzle,
        solution,
    };
    Ok((
        cs.clone(),
        Cat::from_parent_spend(ctx.allocator_mut(), &cs)?.ok_or(DriverError::MissingChild)?,
        Proof::Lineage(LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.p2_puzzle_hash.into(),
            parent_amount: self.coin.amount,
        }),
    ))
}

 */
