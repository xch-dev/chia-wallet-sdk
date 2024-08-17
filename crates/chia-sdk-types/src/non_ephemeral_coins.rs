use std::collections::HashSet;

use chia_protocol::{Coin, CoinSpend};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::Allocator;

use crate::{run_puzzle, Condition, ConditionError};

pub fn non_ephemeral_coins(coin_spends: &[CoinSpend]) -> Result<Vec<Coin>, ConditionError> {
    let mut allocator = Allocator::new();
    let mut created_coins = HashSet::new();

    for coin_spend in coin_spends {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(&mut allocator)?;
        let solution = coin_spend.solution.to_clvm(&mut allocator)?;
        let output = run_puzzle(&mut allocator, puzzle, solution)?;
        let conditions = Vec::<Condition>::from_clvm(&allocator, output)?;

        for condition in conditions {
            if let Condition::CreateCoin(create_coin) = condition {
                created_coins.insert(Coin::new(
                    coin_spend.coin.coin_id(),
                    create_coin.puzzle_hash,
                    create_coin.amount,
                ));
            }
        }
    }

    let non_ephemeral = coin_spends
        .iter()
        .map(|cs| cs.coin)
        .filter(|coin| !created_coins.contains(coin))
        .collect();

    Ok(non_ephemeral)
}

#[cfg(test)]
mod tests {
    use crate::CreateCoin;

    use super::*;

    use chia_protocol::{Bytes32, Program};
    use clvm_traits::{FromClvm, ToClvm};
    use clvmr::NodePtr;

    #[test]
    fn test_non_ephemeral_coins() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        let coins: Vec<Coin> = (0..3)
            .map(|amount| Coin::new(Bytes32::default(), Bytes32::default(), amount))
            .collect();

        let puzzle = 1.to_clvm(&mut allocator)?;
        let puzzle_reveal = Program::from_clvm(&allocator, puzzle)?;
        let identity_solution = Program::from_clvm(&allocator, NodePtr::NIL)?;

        let mut coin_spends = Vec::new();

        for i in 0..3 {
            let create_coin = CreateCoin::new(Bytes32::new([i; 32]), u64::from(i));
            let solution = [&create_coin].to_clvm(&mut allocator)?;

            coin_spends.push(CoinSpend::new(
                Coin::new(
                    coins[i as usize].coin_id(),
                    create_coin.puzzle_hash,
                    create_coin.amount,
                ),
                puzzle_reveal.clone(),
                identity_solution.clone(),
            ));

            coin_spends.push(CoinSpend::new(
                coins[i as usize],
                puzzle_reveal.clone(),
                Program::from_clvm(&allocator, solution)?,
            ));
        }

        let non_ephemeral_coins = non_ephemeral_coins(&coin_spends)?;
        assert_eq!(non_ephemeral_coins, coins);

        Ok(())
    }
}
