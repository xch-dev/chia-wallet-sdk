use chia_consensus::solution_generator::calculate_generator_length;
use chia_protocol::CoinSpend;
use chia_sdk_types::{Condition, run_puzzle_with_cost};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::Allocator;

use crate::DriverError;

const QUOTE_BYTES: usize = 2;

pub fn spend_bundle_cost(coin_spends: &[CoinSpend]) -> Result<u64, DriverError> {
    let mut allocator = Allocator::new();
    let mut cost = 0;

    for coin_spend in coin_spends {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(&mut allocator)?;
        let solution = coin_spend.solution.to_clvm(&mut allocator)?;
        let output = run_puzzle_with_cost(&mut allocator, puzzle, solution, 11_000_000_000, false)?;
        let conditions = Vec::<Condition>::from_clvm(&allocator, output.1)?;

        cost += output.0;

        for condition in conditions {
            if condition.is_agg_sig() {
                cost += 1_200_000;
            } else if condition.is_create_coin() {
                cost += 1_800_000;
            }
        }
    }

    let generator_length_without_quote = calculate_generator_length(coin_spends) - QUOTE_BYTES;
    cost += generator_length_without_quote as u64 * 12_000;

    Ok(cost)
}
