use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_puzzles::standard::{StandardArgs, StandardSolution};
use clvm_traits::clvm_quote;
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{Chainable, ChainedSpend, InnerSpend, SpendContext, SpendError};

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

        let standard_puzzle = ctx.standard_puzzle();

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
    use chia_bls::{sign, Signature};
    use chia_protocol::SpendBundle;
    use chia_puzzles::{standard::STANDARD_PUZZLE_HASH, DeriveSynthetic};
    use clvm_utils::ToTreeHash;
    use clvmr::Allocator;

    use crate::{testing::SECRET_KEY, CreateCoinWithoutMemos, RequiredSignature, WalletSimulator};

    use super::*;

    #[tokio::test]
    async fn test_standard_spend() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let sk = SECRET_KEY.derive_synthetic();
        let pk = sk.public_key();

        let puzzle_hash = CurriedProgram {
            program: STANDARD_PUZZLE_HASH,
            args: StandardArgs { synthetic_key: pk },
        }
        .tree_hash()
        .into();

        let parent = sim.generate_coin(puzzle_hash, 1).await.coin;

        StandardSpend::new()
            .condition(
                ctx.alloc(CreateCoinWithoutMemos {
                    puzzle_hash,
                    amount: 1,
                })
                .unwrap(),
            )
            .finish(&mut ctx, parent, pk)?;

        let coin_spends = ctx.take_spends();

        let required_signatures = RequiredSignature::from_coin_spends(
            &mut allocator,
            &coin_spends,
            WalletSimulator::AGG_SIG_ME.into(),
        )?;

        let mut aggregated_signature = Signature::default();

        for required in required_signatures {
            aggregated_signature += &sign(&sk, required.final_message());
        }

        let ack = peer
            .send_transaction(SpendBundle::new(coin_spends, aggregated_signature))
            .await?;
        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);

        Ok(())
    }
}
