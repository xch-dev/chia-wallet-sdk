use std::cmp::Reverse;

use chia_protocol::Coin;
use indexmap::IndexSet;
use rand::{Rng, RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use thiserror::Error;

/// An error that occurs when selecting coins.
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum CoinSelectionError {
    /// There were no spendable coins to select from.
    #[error("no spendable coins")]
    NoSpendableCoins,

    /// There weren't enough coins to reach the amount. Contains the total amount of the
    /// spendable coins.
    #[error("insufficient balance {0}")]
    InsufficientBalance(u64),

    /// Reaching the amount would require more than 500 coins.
    #[error("exceeded max coins")]
    ExceededMaxCoins,
}

/// Selects a subset of `spendable_coins` whose total amount is at least `amount`.
///
/// Duplicate coins are only counted once. The selection is made in this order:
///
/// 1. A single coin whose amount is exactly `amount`.
/// 2. All coins smaller than `amount`, if they add up to exactly `amount`.
/// 3. The smallest single coin larger than `amount`, if the smaller coins don't add up
///    to `amount`.
/// 4. Otherwise, a knapsack search that tries to minimize the change, falling back to
///    the largest coins until the amount is reached.
///
/// At most 500 coins are selected. The knapsack search uses a fixed seed, so the same
/// input always gives the same result. The returned coins may add up to more than
/// `amount`, in which case the caller should create a change coin for the difference.
///
/// # Errors
///
/// * [`CoinSelectionError::NoSpendableCoins`] if `spendable_coins` is empty.
/// * [`CoinSelectionError::InsufficientBalance`] if the coins add up to less than `amount`.
/// * [`CoinSelectionError::ExceededMaxCoins`] if more than 500 coins would be needed.
///
/// # Example
///
/// ```
/// use chia_protocol::{Bytes32, Coin};
/// use chia_sdk_utils::{CoinSelectionError, select_coins};
///
/// let coin = |amount| Coin::new(Bytes32::new([amount as u8; 32]), Bytes32::default(), amount);
/// let coins = vec![coin(100), coin(200), coin(500)];
///
/// // No single coin or combination of smaller coins equals 400, so the smallest
/// // coin that covers it is selected.
/// assert_eq!(select_coins(coins.clone(), 400)?, vec![coin(500)]);
///
/// // The coins smaller than 300 add up to exactly 300.
/// let mut selected = select_coins(coins.clone(), 300)?;
/// selected.sort_by_key(|coin| coin.amount);
/// assert_eq!(selected, vec![coin(100), coin(200)]);
///
/// assert_eq!(
///     select_coins(coins, 1000),
///     Err(CoinSelectionError::InsufficientBalance(800))
/// );
/// # Ok::<(), CoinSelectionError>(())
/// ```
pub fn select_coins(
    spendable_coins: Vec<Coin>,
    amount: u64,
) -> Result<Vec<Coin>, CoinSelectionError> {
    let amount = u128::from(amount);
    let max_coins = 500;

    // Duplicate coins can only be spent once, so they must not count towards the total.
    let mut spendable_coins: Vec<Coin> = spendable_coins
        .into_iter()
        .collect::<IndexSet<_>>()
        .into_iter()
        .collect();

    // You cannot spend no coins.
    if spendable_coins.is_empty() {
        return Err(CoinSelectionError::NoSpendableCoins);
    }

    // Checks to ensure the balance is sufficient before continuing.
    let spendable_amount = spendable_coins
        .iter()
        .fold(0u128, |acc, coin| acc + u128::from(coin.amount));

    if spendable_amount < amount {
        return Err(CoinSelectionError::InsufficientBalance(
            spendable_amount.try_into().expect("should fit"),
        ));
    }

    // Sorts by amount, descending.
    spendable_coins.sort_unstable_by_key(|coin| Reverse(coin.amount));

    // Exact coin match.
    for coin in &spendable_coins {
        if u128::from(coin.amount) == amount {
            return Ok(vec![*coin]);
        }
    }

    let mut smaller_coins = IndexSet::new();
    let mut smaller_sum = 0;

    for coin in &spendable_coins {
        let coin_amount = u128::from(coin.amount);

        if coin_amount < amount {
            smaller_coins.insert(*coin);
            smaller_sum += coin_amount;
        }
    }

    // Check for an exact match.
    if smaller_sum == amount && smaller_coins.len() <= max_coins && amount != 0 {
        return Ok(smaller_coins.into_iter().collect());
    }

    // There must be a single coin larger than the amount.
    if smaller_sum < amount {
        return Ok(vec![smallest_coin_above(&spendable_coins, amount).unwrap()]);
    }

    // Apply the knapsack algorithm otherwise.
    if smaller_sum > amount {
        if let Some(result) = knapsack_coin_algorithm(
            &mut ChaCha8Rng::seed_from_u64(0),
            &spendable_coins,
            amount,
            u128::MAX,
            max_coins,
        ) {
            return Ok(result.into_iter().collect());
        }

        // Knapsack failed to select coins, so try summing the largest coins.
        let summed_coins = sum_largest_coins(&spendable_coins, amount);

        if summed_coins.len() <= max_coins {
            return Ok(summed_coins.into_iter().collect());
        }

        return Err(CoinSelectionError::ExceededMaxCoins);
    }

    // Try to find a large coin to select.
    if let Some(coin) = smallest_coin_above(&spendable_coins, amount) {
        return Ok(vec![coin]);
    }

    // It would require too many coins to match the amount.
    Err(CoinSelectionError::ExceededMaxCoins)
}

fn sum_largest_coins(coins: &[Coin], amount: u128) -> IndexSet<Coin> {
    let mut selected_coins = IndexSet::new();
    let mut selected_sum = 0;
    for coin in coins {
        selected_sum += u128::from(coin.amount);
        selected_coins.insert(*coin);

        if selected_sum >= amount {
            return selected_coins;
        }
    }
    unreachable!()
}

fn smallest_coin_above(coins: &[Coin], amount: u128) -> Option<Coin> {
    if u128::from(coins[0].amount) < amount {
        return None;
    }
    for coin in coins.iter().rev() {
        if u128::from(coin.amount) >= amount {
            return Some(*coin);
        }
    }
    unreachable!();
}

/// Runs a randomized knapsack search for a set of coins that adds up to at least `amount`
/// with as little excess as possible.
///
/// The search makes 1000 attempts. Each attempt first adds coins at random, then adds the
/// remaining coins in order until the total exceeds `amount`. An exact match is returned as
/// soon as it's found. Otherwise, the set with the smallest total that is above `amount` and
/// below `max_amount` is returned. At most `max_coins` coins are selected in each attempt.
///
/// Returns [`None`] if no attempt found a total in that range. [`select_coins`] calls this
/// with a seeded `rng` so that its result is deterministic.
///
/// # Example
///
/// ```
/// use chia_protocol::{Bytes32, Coin};
/// use chia_sdk_utils::knapsack_coin_algorithm;
/// use rand::SeedableRng;
/// use rand_chacha::ChaCha8Rng;
///
/// let coin = |amount| Coin::new(Bytes32::new([amount as u8; 32]), Bytes32::default(), amount);
/// let coins = vec![coin(50), coin(40), coin(30), coin(20)];
///
/// let mut rng = ChaCha8Rng::seed_from_u64(0);
/// let selected = knapsack_coin_algorithm(&mut rng, &coins, 70, u128::MAX, 500)
///     .expect("a set of coins adds up to 70");
///
/// let total: u64 = selected.iter().map(|coin| coin.amount).sum();
/// assert_eq!(total, 70);
/// ```
pub fn knapsack_coin_algorithm(
    rng: &mut impl Rng,
    spendable_coins: &[Coin],
    amount: u128,
    max_amount: u128,
    max_coins: usize,
) -> Option<IndexSet<Coin>> {
    let mut best_sum = max_amount;
    let mut best_coins = None;

    for _ in 0..1000 {
        let mut selected_coins = IndexSet::new();
        let mut selected_sum = 0;
        let mut target_reached = false;

        for pass in 0..2 {
            if target_reached {
                break;
            }

            for coin in spendable_coins {
                let filter_first = pass != 0 || !rng.random::<bool>();
                let filter_second = pass != 1 || selected_coins.contains(coin);

                if filter_first && filter_second {
                    continue;
                }

                if selected_coins.len() >= max_coins {
                    break;
                }

                if !selected_coins.insert(*coin) {
                    continue;
                }

                selected_sum += u128::from(coin.amount);

                if selected_sum == amount {
                    return Some(selected_coins);
                }

                if selected_sum > amount {
                    target_reached = true;

                    if selected_sum < best_sum {
                        best_sum = selected_sum;
                        best_coins = Some(selected_coins.clone());

                        selected_sum -= u128::from(coin.amount);
                        selected_coins.shift_remove(coin);
                    }
                }
            }
        }
    }

    best_coins
}

#[cfg(test)]
mod tests {
    use chia_protocol::Bytes32;

    use super::*;

    macro_rules! coin_list {
        ( $( $coin:expr ),* $(,)? ) => {
            vec![$( coin($coin) ),*]
        };
    }

    fn coin(amount: u64) -> Coin {
        Coin::new(Bytes32::from([0; 32]), Bytes32::from([0; 32]), amount)
    }

    #[test]
    fn test_select_coins() {
        let coins = coin_list![100, 200, 300, 400, 500];

        // Sorted by amount, ascending.
        let selected = select_coins(coins, 700).unwrap();
        let expected = coin_list![400, 300];
        assert_eq!(selected, expected);
    }

    #[test]
    fn test_insufficient_balance() {
        let coins = coin_list![50, 250, 100_000];

        // Select an amount that is too high.
        let selected = select_coins(coins, 9_999_999);
        assert_eq!(
            selected,
            Err(CoinSelectionError::InsufficientBalance(100_300))
        );
    }

    #[test]
    fn test_no_coins() {
        // There is no amount to select from.
        let selected = select_coins(Vec::new(), 100);
        assert_eq!(selected, Err(CoinSelectionError::NoSpendableCoins));

        // Even if the amount is zero, this should fail.
        let selected = select_coins(Vec::new(), 0);
        assert_eq!(selected, Err(CoinSelectionError::NoSpendableCoins));
    }

    fn unique_coin(index: u32, amount: u64) -> Coin {
        let mut parent = [0; 32];
        parent[..4].copy_from_slice(&index.to_be_bytes());
        Coin::new(Bytes32::from(parent), Bytes32::from([0; 32]), amount)
    }

    #[test]
    fn test_duplicate_coins() {
        let a = unique_coin(0, 100);
        let b = unique_coin(1, 1);

        // The duplicate coin must not count towards the selected amount.
        let selected = select_coins(vec![a, a, b], 150);
        assert_eq!(selected, Err(CoinSelectionError::InsufficientBalance(101)));

        let selected = select_coins(vec![a, a, b], 101).unwrap();
        assert_eq!(selected, vec![a, b]);
    }

    #[test]
    fn test_duplicate_coins_knapsack() {
        let a = unique_coin(0, 100);
        let b = unique_coin(1, 60);

        // Knapsack must not count a duplicate coin twice.
        let selected = knapsack_coin_algorithm(
            &mut ChaCha8Rng::seed_from_u64(0),
            &[a, a, b],
            150,
            u128::MAX,
            500,
        )
        .unwrap();
        let sum: u64 = selected.iter().map(|coin| coin.amount).sum();
        assert!(sum >= 150);
    }

    #[test]
    fn test_max_coins() {
        let coins: Vec<Coin> = (0..600).map(|i| unique_coin(i, 1)).collect();

        let selected = select_coins(coins.clone(), 500).unwrap();
        assert_eq!(selected.len(), 500);

        let selected = select_coins(coins, 501);
        assert_eq!(selected, Err(CoinSelectionError::ExceededMaxCoins));
    }

    #[test]
    fn test_max_coins_exact_match() {
        // All smaller coins sum exactly to the amount, using exactly the max.
        let coins: Vec<Coin> = (0..500).map(|i| unique_coin(i, 1)).collect();
        let selected = select_coins(coins, 500).unwrap();
        assert_eq!(selected.len(), 500);
    }
}
