use chia::{
    clvm_utils::TreeHash,
    protocol::{Bytes32, Coin},
    puzzles::{
        cat::{CatArgs, CatSolution},
        CoinProof, LineageProof,
    },
};
use chia_wallet_sdk::driver::{CatLayer, DriverError, Layer, Spend, SpendContext};
use clvm_traits::ToClvm;
use clvmr::{Allocator, NodePtr};

use crate::{PrecommitLayer, PrecommitLayerSolution};

#[derive(Debug, Clone)]
#[must_use]
pub struct PrecommitCoin<V> {
    pub coin: Coin,
    pub asset_id: Bytes32,
    pub proof: LineageProof,
    pub inner_puzzle_hash: Bytes32,

    pub controller_singleton_struct_hash: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
    pub refund_puzzle_hash: Bytes32,
    pub value: V,
}

impl<V> PrecommitCoin<V> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        proof: LineageProof,
        asset_id: Bytes32,
        controller_singleton_struct_hash: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
        refund_puzzle_hash: Bytes32,
        value: V,
        precommit_amount: u64,
    ) -> Result<Self, DriverError>
    where
        V: ToClvm<Allocator> + Clone,
    {
        let value_ptr = ctx.alloc(&value)?;
        let value_hash = ctx.tree_hash(value_ptr);

        let inner_puzzle_hash = PrecommitLayer::<V>::puzzle_hash(
            controller_singleton_struct_hash,
            relative_block_height,
            payout_puzzle_hash,
            refund_puzzle_hash,
            value_hash,
        );

        Ok(Self {
            coin: Coin::new(
                parent_coin_id,
                CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash).into(),
                precommit_amount,
            ),
            proof,
            asset_id,
            inner_puzzle_hash: inner_puzzle_hash.into(),
            controller_singleton_struct_hash,
            relative_block_height,
            payout_puzzle_hash,
            refund_puzzle_hash,
            value,
        })
    }

    pub fn puzzle_hash(
        asset_id: Bytes32,
        controller_singleton_struct_hash: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
        refund_puzzle_hash: Bytes32,
        value_hash: TreeHash,
    ) -> TreeHash {
        CatArgs::curry_tree_hash(
            asset_id,
            PrecommitLayer::<V>::puzzle_hash(
                controller_singleton_struct_hash,
                relative_block_height,
                payout_puzzle_hash,
                refund_puzzle_hash,
                value_hash,
            ),
        )
    }

    pub fn inner_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError>
    where
        V: Clone + ToClvm<Allocator>,
    {
        PrecommitLayer::<V>::new(
            self.controller_singleton_struct_hash,
            self.relative_block_height,
            self.payout_puzzle_hash,
            self.refund_puzzle_hash,
            self.value.clone(),
        )
        .construct_puzzle(ctx)
    }

    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError>
    where
        V: Clone + ToClvm<Allocator>,
    {
        let inner_puzzle = self.inner_puzzle(ctx)?;

        CatLayer::<NodePtr>::new(self.asset_id, inner_puzzle).construct_puzzle(ctx)
    }

    pub fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        mode: u8,
        singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<NodePtr, DriverError>
    where
        V: ToClvm<Allocator> + Clone,
    {
        let layers = CatLayer::<NodePtr>::new(self.asset_id, self.inner_puzzle(ctx)?);

        let inner_puzzle_solution = ctx.alloc(&PrecommitLayerSolution {
            mode,
            my_amount: self.coin.amount,
            singleton_inner_puzzle_hash,
        })?;

        layers.construct_solution(
            ctx,
            CatSolution {
                inner_puzzle_solution,
                lineage_proof: Some(self.proof),
                prev_coin_id: self.coin.coin_id(),
                this_coin_info: self.coin,
                next_coin_proof: CoinProof {
                    parent_coin_info: self.coin.parent_coin_info,
                    inner_puzzle_hash: self.inner_puzzle_hash,
                    amount: self.coin.amount,
                },
                prev_subtotal: 0,
                extra_delta: 0,
            },
        )
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        mode: u8,
        spender_inner_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError>
    where
        V: ToClvm<Allocator> + Clone,
    {
        let puzzle = self.construct_puzzle(ctx)?;
        let solution = self.construct_solution(ctx, mode, spender_inner_puzzle_hash)?;

        ctx.spend(self.coin, Spend::new(puzzle, solution))
    }
}
