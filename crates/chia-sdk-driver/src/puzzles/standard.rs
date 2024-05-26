use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_puzzles::standard::{StandardArgs, StandardSolution};
use clvm_traits::clvm_quote;
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{
    spend_builder::{InnerSpend, P2Spend, ParentConditions},
    SpendContext, SpendError,
};

#[derive(Debug, Default, Clone)]
pub struct StandardSpend {
    conditions: Vec<NodePtr>,
}

impl StandardSpend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn chain(mut self, chained: ParentConditions) -> Self {
        self.conditions.extend(chained.parent_conditions());
        self
    }

    pub fn inner_spend(
        self,
        ctx: &mut SpendContext<'_>,
        synthetic_key: PublicKey,
    ) -> Result<InnerSpend, SpendError> {
        let standard_puzzle = ctx.standard_puzzle()?;

        let puzzle = ctx.alloc(&CurriedProgram {
            program: standard_puzzle,
            args: StandardArgs { synthetic_key },
        })?;

        let solution = ctx.alloc(&StandardSolution {
            original_public_key: None,
            delegated_puzzle: clvm_quote!(self.conditions),
            solution: (),
        })?;

        Ok(InnerSpend::new(puzzle, solution))
    }

    pub fn finish(
        self,
        ctx: &mut SpendContext<'_>,
        coin: Coin,
        synthetic_key: PublicKey,
    ) -> Result<(), SpendError> {
        let inner_spend = self.inner_spend(ctx, synthetic_key)?;
        let puzzle_reveal = ctx.serialize(&inner_spend.puzzle())?;
        let solution = ctx.serialize(&inner_spend.solution())?;
        ctx.spend(CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }
}

impl P2Spend for StandardSpend {
    fn raw_condition(mut self, condition: NodePtr) -> Self {
        self.conditions.push(condition);
        self
    }
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

        StandardSpend::new()
            .create_coin(ctx, puzzle_hash, 1)?
            .finish(ctx, coin, pk)?;

        test_transaction(&peer, ctx.take_spends(), &[sk]).await;

        Ok(())
    }
}
