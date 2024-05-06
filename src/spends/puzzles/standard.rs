use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_wallet::standard::{StandardArgs, StandardSolution};
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
    ) -> Result<(InnerSpend, Vec<CoinSpend>), SpendError> {
        let standard_puzzle = ctx.standard_puzzle();

        let puzzle = ctx.alloc(CurriedProgram {
            program: standard_puzzle,
            args: StandardArgs { synthetic_key },
        })?;

        let solution = ctx.alloc(standard_solution(self.conditions))?;

        Ok((InnerSpend::new(puzzle, solution), self.coin_spends))
    }

    pub fn finish(
        self,
        ctx: &mut SpendContext,
        coin: Coin,
        synthetic_key: PublicKey,
    ) -> Result<Vec<CoinSpend>, SpendError> {
        let (inner_spend, mut coin_spends) = self.inner_spend(ctx, synthetic_key)?;

        let puzzle_reveal = ctx.serialize(inner_spend.puzzle())?;
        let solution = ctx.serialize(inner_spend.solution())?;
        coin_spends.push(CoinSpend::new(coin, puzzle_reveal, solution));

        Ok(coin_spends)
    }
}

impl Chainable for StandardSpend {
    fn chain(mut self, chained_spend: ChainedSpend) -> Self {
        self.coin_spends.extend(chained_spend.coin_spends);
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
    use chia_bls::derive_keys::master_to_wallet_unhardened;
    use chia_protocol::Bytes32;
    use chia_wallet::{standard::DEFAULT_HIDDEN_PUZZLE_HASH, DeriveSynthetic};
    use clvmr::{serde::node_to_bytes, Allocator};
    use hex_literal::hex;

    use crate::{testing::SECRET_KEY, CreateCoinWithoutMemos};

    use super::*;

    #[test]
    fn test_standard_spend() {
        let synthetic_key = master_to_wallet_unhardened(&SECRET_KEY.public_key(), 0)
            .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);

        let mut a = Allocator::new();
        let mut ctx = SpendContext::new(&mut a);

        let coin = Coin::new(Bytes32::from([0; 32]), Bytes32::from([1; 32]), 42);

        let coin_spend = StandardSpend::new()
            .condition(
                ctx.alloc(CreateCoinWithoutMemos {
                    puzzle_hash: coin.puzzle_hash,
                    amount: coin.amount,
                })
                .unwrap(),
            )
            .finish(&mut ctx, coin, synthetic_key)
            .unwrap()
            .remove(0);

        let output_ptr = coin_spend
            .puzzle_reveal
            .run(&mut a, 0, u64::MAX, &coin_spend.solution)
            .unwrap()
            .1;
        let actual = node_to_bytes(&a, output_ptr).unwrap();

        let expected = hex!(
            "
            ffff32ffb08584adae5630842a1766bc444d2b872dd3080f4e5daaecf6f762a4
            be7dc148f37868149d4217f3dcc9183fe61e48d8bfffa09744e53c76d9ce3c6b
            eb75a3d414ebbec42e31e96621c66b7a832ca1feccceea80ffff33ffa0010101
            0101010101010101010101010101010101010101010101010101010101ff2a80
            80
            "
        );
        assert_eq!(hex::encode(actual), hex::encode(expected));
    }
}
