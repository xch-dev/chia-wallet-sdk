use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_puzzles::standard::{StandardArgs, StandardSolution};
use clvm_traits::clvm_quote;
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{
    spend_builder::{ChainedSpend, InnerSpend, P2Spend},
    SpendContext, SpendError,
};

#[derive(Default)]
pub struct StandardSpend {
    conditions: Vec<NodePtr>,
}

impl StandardSpend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn chain(mut self, chained_spend: ChainedSpend) -> Self {
        self.conditions.extend(chained_spend.parent_conditions());
        self
    }

    pub fn inner_spend(
        self,
        ctx: &mut SpendContext,
        synthetic_key: PublicKey,
    ) -> Result<InnerSpend, SpendError> {
        let standard_puzzle = ctx.standard_puzzle()?;

        let puzzle = ctx.alloc(CurriedProgram {
            program: standard_puzzle,
            args: StandardArgs { synthetic_key },
        })?;

        let solution = ctx.alloc(StandardSolution {
            original_public_key: None,
            delegated_puzzle: clvm_quote!(self.conditions),
            solution: (),
        })?;

        Ok(InnerSpend::new(puzzle, solution))
    }

    pub fn finish(
        self,
        ctx: &mut SpendContext,
        coin: Coin,
        synthetic_key: PublicKey,
    ) -> Result<(), SpendError> {
        let inner_spend = self.inner_spend(ctx, synthetic_key)?;
        let puzzle_reveal = ctx.serialize(inner_spend.puzzle())?;
        let solution = ctx.serialize(inner_spend.solution())?;
        ctx.spend(CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }
}

impl P2Spend for StandardSpend {
    fn raw_condition(&mut self, condition: NodePtr) {
        self.conditions.push(condition);
    }
}

#[cfg(test)]
mod tests {
    use chia_sdk_test::TestWallet;
    use clvmr::Allocator;

    use super::*;

    #[tokio::test]
    async fn test_standard_spend() -> anyhow::Result<()> {
        let mut wallet = TestWallet::new(1).await;
        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        StandardSpend::new()
            .create_coin(ctx, wallet.puzzle_hash, 1)?
            .finish(ctx, wallet.coin, wallet.pk)?;

        wallet.submit(ctx.take_spends()).await?;

        Ok(())
    }
}
