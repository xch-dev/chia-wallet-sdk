use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    cat::{CatArgs, CatSolution, CoinProof, CAT_PUZZLE_HASH},
    LineageProof,
};
use chia_sdk_types::conditions::CreateCoin;
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{Spend, SpendContext, SpendError};

#[derive(Debug, Default)]
#[must_use]
pub struct CatSpend {
    asset_id: Bytes32,
    cat_spends: Vec<CatSpendItem>,
}

#[derive(Debug)]
struct CatSpendItem {
    coin: Coin,
    inner_spend: Spend,
    lineage_proof: LineageProof,
    extra_delta: i64,
}

impl CatSpend {
    pub const fn new(asset_id: Bytes32) -> Self {
        Self {
            asset_id,
            cat_spends: Vec::new(),
        }
    }

    pub fn spend(
        mut self,
        coin: Coin,
        inner_spend: Spend,
        lineage_proof: LineageProof,
        extra_delta: i64,
    ) -> Self {
        self.cat_spends.push(CatSpendItem {
            coin,
            inner_spend,
            lineage_proof,
            extra_delta,
        });
        self
    }

    pub fn finish(self, ctx: &mut SpendContext<'_>) -> Result<(), SpendError> {
        let cat_puzzle_ptr = ctx.cat_puzzle()?;
        let len = self.cat_spends.len();

        let mut total_delta = 0;

        for (index, item) in self.cat_spends.iter().enumerate() {
            let CatSpendItem {
                coin,
                inner_spend,
                lineage_proof,
                extra_delta,
            } = item;

            // Calculate the delta and add it to the subtotal.
            let output = ctx.run(inner_spend.puzzle(), inner_spend.solution())?;
            let conditions: Vec<NodePtr> = ctx.extract(output)?;

            let create_coins = conditions
                .into_iter()
                .filter_map(|ptr| ctx.extract::<CreateCoin>(ptr).ok());

            let delta = create_coins.fold(
                i128::from(coin.amount) - i128::from(*extra_delta),
                |delta, create_coin| delta - i128::from(create_coin.amount),
            );

            let prev_subtotal = total_delta;
            total_delta += delta;

            // Find information of neighboring coins on the ring.
            let prev_cat = &self.cat_spends[if index == 0 { len - 1 } else { index - 1 }];
            let next_cat = &self.cat_spends[if index == len - 1 { 0 } else { index + 1 }];

            let puzzle_reveal = ctx.serialize(&CurriedProgram {
                program: cat_puzzle_ptr,
                args: CatArgs {
                    mod_hash: CAT_PUZZLE_HASH.into(),
                    asset_id: self.asset_id,
                    inner_puzzle: inner_spend.puzzle(),
                },
            })?;

            let solution = ctx.serialize(&CatSolution {
                inner_puzzle_solution: inner_spend.solution(),
                lineage_proof: Some(*lineage_proof),
                prev_coin_id: prev_cat.coin.coin_id(),
                this_coin_info: *coin,
                next_coin_proof: CoinProof {
                    parent_coin_info: next_cat.coin.parent_coin_info,
                    inner_puzzle_hash: ctx.tree_hash(inner_spend.puzzle()).into(),
                    amount: next_cat.coin.amount,
                },
                prev_subtotal: prev_subtotal.try_into()?,
                extra_delta: *extra_delta,
            })?;

            ctx.insert_coin_spend(CoinSpend::new(*coin, puzzle_reveal, solution));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chia_puzzles::{cat::EverythingWithSignatureTailArgs, standard::StandardArgs};
    use chia_sdk_test::{test_transaction, Simulator};
    use chia_sdk_types::conditions::{Condition, RunTail};
    use clvmr::Allocator;

    use crate::{issue_cat_from_coin, issue_cat_from_key, p2_spend, Conditions};

    use super::*;

    #[tokio::test]
    async fn test_cat_spend_multi() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = sim.secret_key().await?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 6).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (issue_cat, issuance) = issue_cat_from_coin(
            ctx,
            coin.coin_id(),
            6,
            Conditions::new()
                .create_hinted_coin(puzzle_hash, 1, puzzle_hash)
                .create_hinted_coin(puzzle_hash, 2, puzzle_hash)
                .create_hinted_coin(puzzle_hash, 3, puzzle_hash),
        )?;

        ctx.spend_p2_coin(coin, pk, issue_cat)?;

        let cat_puzzle_hash =
            CatArgs::curry_tree_hash(issuance.asset_id, puzzle_hash.into()).into();

        CatSpend::new(issuance.asset_id)
            .spend(
                Coin::new(issuance.eve_coin.coin_id(), cat_puzzle_hash, 1),
                p2_spend(
                    ctx,
                    pk,
                    Conditions::new().create_hinted_coin(puzzle_hash, 1, puzzle_hash),
                )?,
                issuance.lineage_proof,
                0,
            )
            .spend(
                Coin::new(issuance.eve_coin.coin_id(), cat_puzzle_hash, 2),
                p2_spend(
                    ctx,
                    pk,
                    Conditions::new().create_hinted_coin(puzzle_hash, 2, puzzle_hash),
                )?,
                issuance.lineage_proof,
                0,
            )
            .spend(
                Coin::new(issuance.eve_coin.coin_id(), cat_puzzle_hash, 3),
                p2_spend(
                    ctx,
                    pk,
                    Conditions::new().create_hinted_coin(puzzle_hash, 3, puzzle_hash),
                )?,
                issuance.lineage_proof,
                0,
            )
            .finish(ctx)?;

        test_transaction(
            &peer,
            ctx.take_spends(),
            &[sk],
            sim.config().genesis_challenge,
        )
        .await;

        Ok(())
    }

    #[tokio::test]
    async fn test_cat_spend() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = sim.secret_key().await?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 1).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (issue_cat, issuance) = issue_cat_from_coin(
            ctx,
            coin.coin_id(),
            1,
            Conditions::new().create_hinted_coin(puzzle_hash, 1, puzzle_hash),
        )?;

        ctx.spend_p2_coin(coin, pk, issue_cat)?;

        let cat_puzzle_hash =
            CatArgs::curry_tree_hash(issuance.asset_id, puzzle_hash.into()).into();
        let cat_coin = Coin::new(issuance.eve_coin.coin_id(), cat_puzzle_hash, 1);

        CatSpend::new(issuance.asset_id)
            .spend(
                cat_coin,
                p2_spend(
                    ctx,
                    pk,
                    Conditions::new().create_hinted_coin(puzzle_hash, 1, puzzle_hash),
                )?,
                issuance.lineage_proof,
                0,
            )
            .finish(ctx)?;

        test_transaction(
            &peer,
            ctx.take_spends(),
            &[sk],
            sim.config().genesis_challenge,
        )
        .await;

        Ok(())
    }

    #[tokio::test]
    async fn test_cat_melt() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = sim.secret_key().await?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 10000).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (issue_cat, issuance) = issue_cat_from_key(
            ctx,
            coin.coin_id(),
            pk,
            10000,
            Conditions::new().create_hinted_coin(puzzle_hash, 10000, puzzle_hash),
        )?;

        ctx.spend_p2_coin(coin, pk, issue_cat)?;

        let tail = ctx.everything_with_signature_tail_puzzle()?;
        let tail_program = ctx.alloc(&CurriedProgram {
            program: tail,
            args: EverythingWithSignatureTailArgs::new(pk),
        })?;
        let run_tail = Condition::Other(ctx.alloc(&RunTail::new(tail_program, ()))?);

        let inner_spend = p2_spend(
            ctx,
            pk,
            Conditions::new()
                .create_hinted_coin(puzzle_hash, 7000, puzzle_hash)
                .condition(run_tail),
        )?;

        let cat_puzzle_hash =
            CatArgs::curry_tree_hash(issuance.asset_id, puzzle_hash.into()).into();
        let cat_coin = Coin::new(issuance.eve_coin.coin_id(), cat_puzzle_hash, 10000);

        CatSpend::new(issuance.asset_id)
            .spend(cat_coin, inner_spend, issuance.lineage_proof, -3000)
            .finish(ctx)?;

        test_transaction(
            &peer,
            ctx.take_spends(),
            &[sk],
            sim.config().genesis_challenge,
        )
        .await;

        Ok(())
    }
}
