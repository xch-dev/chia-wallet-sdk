use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::{
    cat::{CatSolution, CoinProof},
    LineageProof, Proof,
};
use chia_sdk_types::conditions::{run_puzzle, Condition};
use clvm_traits::{FromClvm, FromNodePtr, ToNodePtr};
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{CatLayer, DriverError, Layer, Spend, SpendContext, TransparentLayer};

#[derive(Debug, Clone, Copy)]
pub struct Cat {
    pub coin: Coin,

    pub asset_id: Bytes32,

    // innermost (owner) layer
    pub p2_puzzle_hash: TreeHash,
    pub p2_puzzle: Option<NodePtr>,
}

impl Cat {
    pub fn new(
        coin: Coin,
        asset_id: Bytes32,
        p2_puzzle_hash: TreeHash,
        p2_puzzle: Option<NodePtr>,
    ) -> Self {
        Cat {
            coin,
            asset_id,
            p2_puzzle_hash,
            p2_puzzle,
        }
    }

    #[must_use]
    pub fn with_coin(mut self, coin: Coin) -> Self {
        self.coin = coin;
        self
    }

    #[must_use]
    pub fn with_p2_puzzle(mut self, p2_puzzle: NodePtr) -> Self {
        self.p2_puzzle = Some(p2_puzzle);
        self
    }

    pub fn from_parent_spend(
        allocator: &mut Allocator,
        cs: &CoinSpend,
    ) -> Result<Option<Self>, DriverError> {
        let puzzle_ptr = cs
            .puzzle_reveal
            .to_node_ptr(allocator)
            .map_err(DriverError::ToClvm)?;
        let solution_ptr = cs
            .solution
            .to_node_ptr(allocator)
            .map_err(DriverError::ToClvm)?;

        let res =
            CatLayer::<TransparentLayer>::from_parent_spend(allocator, puzzle_ptr, solution_ptr)?;

        let output = run_puzzle(allocator, puzzle_ptr, solution_ptr).map_err(DriverError::Eval)?;
        let conditions =
            Vec::<Condition>::from_clvm(allocator, output).map_err(DriverError::FromClvm)?;

        let create_coin = conditions
            .into_iter()
            .find(|cond| matches!(cond, Condition::CreateCoin(_)))
            .ok_or(DriverError::MissingParentCreateCoin)?;
        let (amount, puzzle_hash) = if let Condition::CreateCoin(create_coin) = create_coin {
            (create_coin.amount, create_coin.puzzle_hash)
        } else {
            unreachable!()
        };

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(Cat {
                coin: Coin::new(cs.coin.coin_id(), puzzle_hash, amount),
                asset_id: res.asset_id,
                p2_puzzle_hash: res.inner_puzzle.puzzle_hash,
                p2_puzzle: res.inner_puzzle.puzzle,
            })),
        }
    }

    pub fn from_puzzle(
        allocator: &mut Allocator,
        coin: Coin,
        puzzle: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let res = CatLayer::<TransparentLayer>::from_puzzle(allocator, puzzle)?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(Cat {
                coin,
                asset_id: res.asset_id,
                p2_puzzle_hash: res.inner_puzzle.puzzle_hash,
                p2_puzzle: res.inner_puzzle.puzzle,
            })),
        }
    }

    pub fn get_layered_object(&self, p2_puzzle: Option<NodePtr>) -> CatLayer<TransparentLayer> {
        CatLayer {
            asset_id: self.asset_id,
            inner_puzzle: TransparentLayer {
                puzzle_hash: self.p2_puzzle_hash,
                puzzle: match self.p2_puzzle {
                    Some(p2_puzzle) => Some(p2_puzzle),
                    None => p2_puzzle,
                },
            },
        }
    }

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
            Program::from_node_ptr(ctx.allocator(), puzzle_ptr).map_err(DriverError::FromClvm)?;

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
            Program::from_node_ptr(ctx.allocator(), solution_ptr).map_err(DriverError::FromClvm)?;

        let cs = CoinSpend {
            coin: self.coin,
            puzzle_reveal: puzzle,
            solution,
        };
        Ok((
            cs.clone(),
            Cat::from_parent_spend(ctx.allocator_mut(), &cs)?.ok_or(DriverError::MissingChild)?,
            Proof::Lineage(LineageProof {
                parent_parent_coin_id: self.coin.parent_coin_info,
                parent_inner_puzzle_hash: self.p2_puzzle_hash.into(),
                parent_amount: self.coin.amount,
            }),
        ))
    }
}
