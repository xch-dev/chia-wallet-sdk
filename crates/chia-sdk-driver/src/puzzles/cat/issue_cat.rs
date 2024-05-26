use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    cat::{
        CatArgs, CatSolution, CoinProof, EverythingWithSignatureTailArgs, GenesisByCoinIdTailArgs,
        CAT_PUZZLE_HASH,
    },
    LineageProof,
};
use chia_sdk_types::conditions::RunTail;
use clvm_traits::clvm_quote;
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{
    spend_builder::{P2Spend, ParentConditions},
    SpendContext, SpendError,
};

#[derive(Debug)]
pub struct IssueCat {
    parent_coin_id: Bytes32,
    conditions: Vec<NodePtr>,
}

#[derive(Debug, Clone, Copy)]
pub struct CatIssuanceInfo {
    pub asset_id: Bytes32,
    pub lineage_proof: LineageProof,
    pub eve_coin: Coin,
}

impl IssueCat {
    #[must_use]
    pub const fn new(parent_coin_id: Bytes32) -> Self {
        Self {
            parent_coin_id,
            conditions: Vec::new(),
        }
    }

    pub fn single_issuance(
        self,
        ctx: &mut SpendContext<'_>,
        amount: u64,
    ) -> Result<(ParentConditions, CatIssuanceInfo), SpendError> {
        let tail_puzzle_ptr = ctx.genesis_by_coin_id_tail_puzzle()?;

        let tail = ctx.alloc(&CurriedProgram {
            program: tail_puzzle_ptr,
            args: GenesisByCoinIdTailArgs {
                genesis_coin_id: self.parent_coin_id,
            },
        })?;
        let asset_id = ctx.tree_hash(tail).into();

        self.raw_condition(ctx.alloc(&RunTail {
            program: tail,
            solution: NodePtr::NIL,
        })?)
        .finish_raw(ctx, asset_id, amount)
    }

    pub fn multi_issuance(
        self,
        ctx: &mut SpendContext<'_>,
        public_key: PublicKey,
        amount: u64,
    ) -> Result<(ParentConditions, CatIssuanceInfo), SpendError> {
        let tail_puzzle_ptr = ctx.everything_with_signature_tail_puzzle()?;

        let tail = ctx.alloc(&CurriedProgram {
            program: tail_puzzle_ptr,
            args: EverythingWithSignatureTailArgs { public_key },
        })?;
        let asset_id = ctx.tree_hash(tail).into();

        self.raw_condition(ctx.alloc(&RunTail {
            program: tail,
            solution: NodePtr::NIL,
        })?)
        .finish_raw(ctx, asset_id, amount)
    }

    pub fn finish_raw(
        self,
        ctx: &mut SpendContext<'_>,
        asset_id: Bytes32,
        amount: u64,
    ) -> Result<(ParentConditions, CatIssuanceInfo), SpendError> {
        let cat_puzzle_ptr = ctx.cat_puzzle()?;

        let inner_puzzle = ctx.alloc(&clvm_quote!(self.conditions))?;
        let inner_puzzle_hash = ctx.tree_hash(inner_puzzle).into();

        let puzzle = ctx.alloc(&CurriedProgram {
            program: cat_puzzle_ptr,
            args: CatArgs {
                mod_hash: CAT_PUZZLE_HASH.into(),
                asset_id,
                inner_puzzle,
            },
        })?;

        let puzzle_hash = ctx.tree_hash(puzzle).into();
        let eve_coin = Coin::new(self.parent_coin_id, puzzle_hash, amount);

        let solution = ctx.serialize(&CatSolution {
            inner_puzzle_solution: (),
            lineage_proof: None,
            prev_coin_id: eve_coin.coin_id(),
            this_coin_info: eve_coin,
            next_coin_proof: CoinProof {
                parent_coin_info: self.parent_coin_id,
                inner_puzzle_hash,
                amount,
            },
            prev_subtotal: 0,
            extra_delta: 0,
        })?;

        let puzzle_reveal = ctx.serialize(&puzzle)?;
        ctx.spend(CoinSpend::new(eve_coin, puzzle_reveal, solution));

        let chained_spend =
            ParentConditions::new().create_hinted_coin(ctx, puzzle_hash, amount, puzzle_hash)?;

        let issuance_info = CatIssuanceInfo {
            asset_id,
            lineage_proof: LineageProof {
                parent_parent_coin_id: eve_coin.parent_coin_info,
                parent_inner_puzzle_hash: inner_puzzle_hash,
                parent_amount: eve_coin.amount,
            },
            eve_coin,
        };

        Ok((chained_spend, issuance_info))
    }
}

impl P2Spend for IssueCat {
    fn raw_condition(mut self, condition: NodePtr) -> Self {
        self.conditions.push(condition);
        self
    }
}

#[cfg(test)]
mod tests {
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{test_transaction, Simulator};
    use clvmr::Allocator;

    use crate::puzzles::StandardSpend;

    use super::*;

    #[tokio::test]
    async fn test_single_issuance_cat() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = sim.secret_key().await?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 1).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (issue_cat, _cat_info) = IssueCat::new(coin.coin_id())
            .create_hinted_coin(ctx, puzzle_hash, 1, puzzle_hash)?
            .single_issuance(ctx, 1)?;

        StandardSpend::new()
            .chain(issue_cat)
            .finish(ctx, coin, pk)?;

        test_transaction(&peer, ctx.take_spends(), &[sk]).await;

        Ok(())
    }

    #[tokio::test]
    async fn test_multi_issuance_cat() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = sim.secret_key().await?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 1).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (issue_cat, _cat_info) = IssueCat::new(coin.coin_id())
            .create_hinted_coin(ctx, puzzle_hash, 1, puzzle_hash)?
            .multi_issuance(ctx, pk, 1)?;

        StandardSpend::new()
            .chain(issue_cat)
            .finish(ctx, coin, pk)?;

        test_transaction(&peer, ctx.take_spends(), &[sk]).await;

        Ok(())
    }
}
