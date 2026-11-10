use chia_consensus::solution_generator::calculate_generator_length;
use chia_protocol::CoinSpend;
use chia_sdk_types::{Condition, run_puzzle_with_cost};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::Allocator;

use crate::DriverError;

const QUOTE_BYTES: usize = 2;

/// Estimates the CLVM cost of a spend bundle made of the given coin spends.
///
/// The estimate is the sum of:
///
/// - the execution cost of each puzzle with its solution (with an 11 billion cost limit),
/// - 1,200,000 for each `AGG_SIG_*` condition and 1,800,000 for each `CREATE_COIN` condition,
/// - 12,000 for each byte of the serialized generator, excluding the quote.
///
/// This doesn't account for every condition cost charged by consensus, so it's an approximation
/// that is useful for calculating fees.
///
/// Returns an error if a puzzle fails to run or its output isn't a list of conditions.
///
/// # Example
///
/// ```
/// use chia_sdk_driver::{SpendContext, StandardLayer, spend_bundle_cost};
/// use chia_sdk_test::Simulator;
/// use chia_sdk_types::Conditions;
/// use chia_puzzle_types::Memos;
///
/// let mut sim = Simulator::new();
/// let mut ctx = SpendContext::new();
/// let alice = sim.bls(1);
///
/// StandardLayer::new(alice.pk).spend(
///     &mut ctx,
///     alice.coin,
///     Conditions::new().create_coin(alice.puzzle_hash, alice.coin.amount, Memos::None),
/// )?;
///
/// let cost = spend_bundle_cost(&ctx.take())?;
/// assert!(cost > 1_200_000 + 1_800_000);
/// # Ok::<(), anyhow::Error>(())
/// ```
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
