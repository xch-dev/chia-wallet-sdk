use chia_bls::PublicKey;
use chia_puzzles::standard::{StandardArgs, StandardSolution};
use clvm_utils::CurriedProgram;

use crate::{Conditions, Spend, SpendContext, SpendError};

pub fn p2_spend(
    ctx: &mut SpendContext<'_>,
    synthetic_key: PublicKey,
    conditions: Conditions,
) -> Result<Spend, SpendError> {
    let standard_puzzle = ctx.standard_puzzle()?;

    let puzzle = ctx.alloc(&CurriedProgram {
        program: standard_puzzle,
        args: StandardArgs::new(synthetic_key),
    })?;

    let solution = ctx.alloc(&StandardSolution::from_conditions(conditions))?;

    Ok(Spend::new(puzzle, solution))
}

#[cfg(test)]
mod tests {
    use chia_sdk_test::{test_transaction, Simulator};
    use clvmr::Allocator;

    use super::*;

    #[tokio::test]
    async fn test_standard_spend() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = sim.secret_key().await?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 1).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        ctx.spend_p2_coin(coin, pk, Conditions::new().create_coin(puzzle_hash, 1))?;

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
