use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_wallet::standard::{StandardArgs, StandardSolution};
use clvm_traits::{clvm_quote, ToClvm};
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{BaseSpend, ChainedSpend, SpendContext, SpendError};

pub struct StandardSpend<'a, 'b> {
    ctx: &'a mut SpendContext<'b>,
    coin: Coin,
    coin_spends: Vec<CoinSpend>,
    conditions: Vec<NodePtr>,
}

impl<'a, 'b> StandardSpend<'a, 'b> {
    pub fn new(ctx: &'a mut SpendContext<'b>, coin: Coin) -> Self {
        Self {
            ctx,
            coin,
            coin_spends: Vec::with_capacity(1),
            conditions: Vec::new(),
        }
    }

    pub fn finish(mut self, synthetic_key: PublicKey) -> Result<Vec<CoinSpend>, SpendError> {
        let coin_spend = spend_standard_coin(self.ctx, self.coin, synthetic_key, self.conditions)?;
        self.coin_spends.push(coin_spend);
        Ok(self.coin_spends)
    }
}

impl<'a, 'b> BaseSpend for StandardSpend<'a, 'b> {
    fn chain(mut self, chained_spend: ChainedSpend) -> Self {
        self.conditions.extend(chained_spend.parent_conditions);
        self.coin_spends.extend(chained_spend.coin_spends);
        self
    }

    fn condition(mut self, condition: impl ToClvm<NodePtr>) -> Result<Self, SpendError> {
        self.conditions.push(self.ctx.alloc(condition)?);
        Ok(self)
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

/// Creates a new coin spend for a given standard transaction coin.
pub fn spend_standard_coin<T>(
    ctx: &mut SpendContext,
    coin: Coin,
    synthetic_key: PublicKey,
    conditions: T,
) -> Result<CoinSpend, SpendError>
where
    T: ToClvm<NodePtr>,
{
    let standard_puzzle = ctx.standard_puzzle();

    let puzzle_reveal = ctx.serialize(CurriedProgram {
        program: standard_puzzle,
        args: StandardArgs { synthetic_key },
    })?;
    let solution = ctx.alloc(standard_solution(conditions))?;
    let serialized_solution = ctx.serialize(solution)?;

    Ok(CoinSpend::new(coin, puzzle_reveal, serialized_solution))
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
        let puzzle_hash = coin.puzzle_hash;
        let amount = coin.amount;

        let coin_spend = spend_standard_coin(
            &mut ctx,
            coin,
            synthetic_key,
            [CreateCoinWithoutMemos {
                puzzle_hash,
                amount,
            }],
        )
        .unwrap();
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
