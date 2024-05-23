use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    cat::{CatArgs, CatSolution, CoinProof, EverythingWithSignatureTailArgs, CAT_PUZZLE_HASH},
    LineageProof,
};
use chia_sdk_types::conditions::*;
use clvm_traits::clvm_quote;
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{
    spend_builder::{P2Spend, ParentConditions},
    SpendContext, SpendError,
};

pub struct IssueCat {
    parent_coin_id: Bytes32,
    conditions: Vec<NodePtr>,
}

pub struct CatIssuanceInfo {
    pub asset_id: Bytes32,
    pub lineage_proof: LineageProof,
    pub eve_coin: Coin,
}

impl IssueCat {
    pub fn new(parent_coin_id: Bytes32) -> Self {
        Self {
            parent_coin_id,
            conditions: Vec::new(),
        }
    }

    pub fn multi_issuance(
        self,
        ctx: &mut SpendContext,
        public_key: PublicKey,
        amount: u64,
    ) -> Result<(ParentConditions, CatIssuanceInfo), SpendError> {
        let tail_puzzle_ptr = ctx.everything_with_signature_tail_puzzle()?;

        let tail = ctx.alloc(CurriedProgram {
            program: tail_puzzle_ptr,
            args: EverythingWithSignatureTailArgs { public_key },
        })?;
        let asset_id = ctx.tree_hash(tail).into();

        self.raw_condition(ctx.alloc(RunTail {
            program: tail,
            solution: NodePtr::NIL,
        })?)
        .finish_raw(ctx, asset_id, amount)
    }

    pub fn finish_raw(
        self,
        ctx: &mut SpendContext,
        asset_id: Bytes32,
        amount: u64,
    ) -> Result<(ParentConditions, CatIssuanceInfo), SpendError> {
        let cat_puzzle_ptr = ctx.cat_puzzle()?;

        let inner_puzzle = ctx.alloc(clvm_quote!(self.conditions))?;
        let inner_puzzle_hash = ctx.tree_hash(inner_puzzle).into();

        let puzzle = ctx.alloc(CurriedProgram {
            program: cat_puzzle_ptr,
            args: CatArgs {
                mod_hash: CAT_PUZZLE_HASH.into(),
                asset_id,
                inner_puzzle,
            },
        })?;

        let puzzle_hash = ctx.tree_hash(puzzle).into();
        let eve_coin = Coin::new(self.parent_coin_id, puzzle_hash, amount);

        let solution = ctx.serialize(CatSolution {
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

        let puzzle_reveal = ctx.serialize(puzzle)?;
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
    use chia_sdk_test::TestWallet;
    use clvmr::Allocator;

    use crate::puzzles::StandardSpend;

    use super::*;

    #[tokio::test]
    async fn test_cat_issuance() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);
        let mut wallet = TestWallet::new(1).await;

        let (issue_cat, _cat_info) = IssueCat::new(wallet.coin.coin_id())
            .create_hinted_coin(ctx, wallet.puzzle_hash, 1, wallet.puzzle_hash)?
            .multi_issuance(ctx, wallet.pk, 1)?;

        StandardSpend::new()
            .chain(issue_cat)
            .finish(ctx, wallet.coin, wallet.pk)?;

        wallet.submit(ctx.take_spends()).await?;

        Ok(())
    }
}
