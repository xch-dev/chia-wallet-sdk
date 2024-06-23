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
use clvm_traits::{clvm_quote, ToClvm};
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{Conditions, SpendContext, SpendError};

#[derive(Debug, Clone, Copy)]
pub struct IssueCat {
    pub asset_id: Bytes32,
    pub lineage_proof: LineageProof,
    pub eve_coin: Coin,
}

pub fn issue_cat_from_coin(
    ctx: &mut SpendContext,
    parent_coin_id: Bytes32,
    amount: u64,
    extra_conditions: Conditions,
) -> Result<(Conditions, IssueCat), SpendError> {
    let tail_puzzle_ptr = ctx.genesis_by_coin_id_tail_puzzle()?;
    let tail = ctx.alloc(&CurriedProgram {
        program: tail_puzzle_ptr,
        args: GenesisByCoinIdTailArgs::new(parent_coin_id),
    })?;
    let asset_id = ctx.tree_hash(tail).into();

    issue_cat(
        ctx,
        parent_coin_id,
        asset_id,
        amount,
        RunTail::new(tail, ()),
        extra_conditions,
    )
}

pub fn issue_cat_from_key(
    ctx: &mut SpendContext,
    parent_coin_id: Bytes32,
    public_key: PublicKey,
    amount: u64,
    extra_conditions: Conditions,
) -> Result<(Conditions, IssueCat), SpendError> {
    let tail_puzzle_ptr = ctx.everything_with_signature_tail_puzzle()?;
    let tail = ctx.alloc(&CurriedProgram {
        program: tail_puzzle_ptr,
        args: EverythingWithSignatureTailArgs::new(public_key),
    })?;
    let asset_id = ctx.tree_hash(tail).into();

    issue_cat(
        ctx,
        parent_coin_id,
        asset_id,
        amount,
        RunTail::new(tail, ()),
        extra_conditions,
    )
}

pub fn issue_cat<P, S>(
    ctx: &mut SpendContext,
    parent_coin_id: Bytes32,
    asset_id: Bytes32,
    amount: u64,
    run_tail: RunTail<P, S>,
    extra_conditions: Conditions,
) -> Result<(Conditions, IssueCat), SpendError>
where
    P: ToClvm<NodePtr>,
    S: ToClvm<NodePtr>,
{
    let cat_puzzle_ptr = ctx.cat_puzzle()?;

    let inner_puzzle = ctx.alloc(&clvm_quote!((run_tail, extra_conditions)))?;
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
    let eve_coin = Coin::new(parent_coin_id, puzzle_hash, amount);

    let solution = ctx.serialize(&CatSolution {
        inner_puzzle_solution: (),
        lineage_proof: None,
        prev_coin_id: eve_coin.coin_id(),
        this_coin_info: eve_coin,
        next_coin_proof: CoinProof {
            parent_coin_info: parent_coin_id,
            inner_puzzle_hash,
            amount,
        },
        prev_subtotal: 0,
        extra_delta: 0,
    })?;

    let puzzle_reveal = ctx.serialize(&puzzle)?;
    ctx.insert_coin_spend(CoinSpend::new(eve_coin, puzzle_reveal, solution));

    let chained_spend = Conditions::new().create_hinted_coin(puzzle_hash, amount, puzzle_hash);

    let issuance_info = IssueCat {
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

#[cfg(test)]
mod tests {
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{secret_key, test_transaction, Simulator};

    use super::*;

    #[tokio::test]
    async fn test_single_issuance_cat() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 1).await;

        let conditions = Conditions::new().create_hinted_coin(puzzle_hash, 1, puzzle_hash);
        let (issue_cat, _cat_info) = issue_cat_from_coin(ctx, coin.coin_id(), 1, conditions)?;

        ctx.spend_p2_coin(coin, pk, issue_cat)?;

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
    async fn test_multi_issuance_cat() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 1).await;

        let conditions = Conditions::new().create_hinted_coin(puzzle_hash, 1, puzzle_hash);
        let (issue_cat, _cat_info) = issue_cat_from_key(ctx, coin.coin_id(), pk, 1, conditions)?;

        ctx.spend_p2_coin(coin, pk, issue_cat)?;

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
