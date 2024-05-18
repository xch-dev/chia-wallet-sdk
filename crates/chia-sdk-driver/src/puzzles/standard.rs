use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_puzzles::standard::{StandardArgs, StandardSolution};
use clvm_traits::clvm_quote;
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{
    spend_builder::{Chainable, ChainedSpend, InnerSpend},
    SpendContext, SpendError,
};

/// ## Example
///
/// Here is an example of spending a standard p2 coin:
///
/// ```
/// # use chia_bls::PublicKey;
/// # use chia_protocol::{Bytes32, Coin};
/// # use clvmr::Allocator;
/// # use chia_wallet_sdk::*;
/// # let mut allocator = Allocator::new();
/// # let mut ctx = SpendContext::new(&mut allocator);
/// # let public_key = PublicKey::default();
/// # let puzzle_hash = Bytes32::default();
/// # let coin = Coin::new(Bytes32::default(), Bytes32::default(), 1);
/// let create_coin = CreateCoinWithoutMemos { puzzle_hash, amount: 1 };
///
/// StandardSpend::new()
///     .condition(ctx.alloc(create_coin).unwrap())
///     .finish(&mut ctx, coin, public_key)
///     .unwrap()
/// ```
#[derive(Default)]
pub struct StandardSpend {
    coin_spends: Vec<CoinSpend>,
    conditions: Vec<NodePtr>,
}

impl StandardSpend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inner_spend(
        self,
        ctx: &mut SpendContext,
        synthetic_key: PublicKey,
    ) -> Result<InnerSpend, SpendError> {
        for coin_spend in self.coin_spends {
            ctx.spend(coin_spend);
        }

        let standard_puzzle = ctx.standard_puzzle()?;

        let puzzle = ctx.alloc(CurriedProgram {
            program: standard_puzzle,
            args: StandardArgs { synthetic_key },
        })?;

        let solution = ctx.alloc(standard_solution(self.conditions))?;

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

impl Chainable for StandardSpend {
    fn chain(mut self, chained_spend: ChainedSpend) -> Self {
        self.conditions.extend(chained_spend.parent_conditions);
        self
    }

    fn condition(mut self, condition: NodePtr) -> Self {
        self.conditions.push(condition);
        self
    }
}

/// Constructs a solution for the standard puzzle, given a list of condition.
/// This assumes no hidden puzzle is being used in this spend.
pub fn standard_solution<T>(conditions: T) -> StandardSolution<(u8, T), ()> {
    StandardSolution {
        original_public_key: None,
        delegated_puzzle: clvm_quote!(conditions),
        solution: (),
    }
}

#[cfg(test)]
mod tests {
    use chia_sdk_test::TestWallet;
    use chia_sdk_types::conditions::CreateCoinWithoutMemos;
    use clvmr::Allocator;

    use super::*;

    #[tokio::test]
    async fn test_standard_spend() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);
        let mut wallet = TestWallet::new(1).await;

        StandardSpend::new()
            .condition(
                ctx.alloc(CreateCoinWithoutMemos {
                    puzzle_hash: wallet.puzzle_hash,
                    amount: 1,
                })
                .unwrap(),
            )
            .finish(ctx, wallet.coin, wallet.pk)?;

        wallet.submit(ctx.take_spends()).await?;

        Ok(())
    }
}
