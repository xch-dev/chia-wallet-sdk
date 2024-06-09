use std::collections::HashSet;

use chia_protocol::{Coin, CoinSpend};
use chia_sdk_types::conditions::Condition;
use clvm_traits::{FromClvm, ToNodePtr};
use clvmr::{reduction::Reduction, Allocator, NodePtr};

use crate::ParseError;

pub fn parse_conditions(
    allocator: &mut Allocator,
    conditions: NodePtr,
) -> Result<Vec<Condition<NodePtr>>, ParseError> {
    Vec::<NodePtr>::from_clvm(allocator, conditions)?
        .into_iter()
        .map(|condition| Ok(Condition::from_clvm(allocator, condition)?))
        .collect()
}

pub fn run_puzzle(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<NodePtr, ParseError> {
    let Reduction(_cost, output) = clvmr::run_program(
        allocator,
        &clvmr::ChiaDialect::new(0),
        puzzle,
        solution,
        11_000_000_000,
    )?;
    Ok(output)
}

pub fn puzzle_conditions(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<Vec<Condition<NodePtr>>, ParseError> {
    let output = run_puzzle(allocator, puzzle, solution)?;
    parse_conditions(allocator, output)
}

pub fn non_ephemeral_coins(coin_spends: &[CoinSpend]) -> Result<Vec<Coin>, ParseError> {
    let mut allocator = Allocator::new();
    let mut created_coins = HashSet::new();

    for coin_spend in coin_spends {
        let puzzle = coin_spend.puzzle_reveal.to_node_ptr(&mut allocator)?;
        let solution = coin_spend.solution.to_node_ptr(&mut allocator)?;
        let conditions = puzzle_conditions(&mut allocator, puzzle, solution)?;

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
    use chia_protocol::{Bytes32, Program};
    use chia_sdk_types::conditions::CreateCoin;
    use clvm_traits::{FromNodePtr, ToClvm};

    use super::*;

    #[test]
    fn test_non_ephemeral_coins() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        let coins: Vec<Coin> = (0..3)
            .map(|amount| Coin::new(Bytes32::default(), Bytes32::default(), amount))
            .collect();

        let puzzle = 1.to_clvm(&mut allocator)?;
        let puzzle_reveal = Program::from_node_ptr(&allocator, puzzle)?;
        let identity_solution = Program::from_node_ptr(&allocator, NodePtr::NIL)?;

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
                Program::from_node_ptr(&allocator, solution)?,
            ));
        }

        let non_ephemeral_coins = non_ephemeral_coins(&coin_spends)?;
        assert_eq!(non_ephemeral_coins, coins);

        Ok(())
    }
}
