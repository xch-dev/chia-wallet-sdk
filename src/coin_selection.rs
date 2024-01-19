use chia_protocol::Coin;
use thiserror::Error;

/// Simple methods of coin selection to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoinSelectionMode {
    /// Selects coins from largest to smallest until the amount is reached.
    Largest,
    /// Selects coins from smallest to largest until the amount is reached.
    Smallest,
}

/// An error that occurs when selecting coins.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CoinSelectionError {
    /// There were no coins to select from.
    #[error("no spendable coins")]
    NoSpendableCoins,

    /// There weren't enough coins to reach the amount.
    #[error("insufficient balance {0}")]
    InsufficientBalance(u64),
}

/// This is not the most efficient coin selection method.
/// It naively selects coins until an amount is reached.
/// This should be replaced with a more thorough algorithm later.
pub fn select_coins(
    mut coins: Vec<Coin>,
    amount: u64,
    mode: CoinSelectionMode,
) -> Result<Vec<Coin>, CoinSelectionError> {
    // You cannot select no coins, even if the target amount is zero.
    // This is because for Chia you need to spend a coin to create one.
    if coins.is_empty() {
        return Err(CoinSelectionError::NoSpendableCoins);
    }

    // Sort by either largest or smallest coins first.
    match mode {
        CoinSelectionMode::Largest => {
            coins.sort_by(|a, b| b.amount.cmp(&a.amount));
        }
        CoinSelectionMode::Smallest => {
            coins.sort_by(|a, b| a.amount.cmp(&b.amount));
        }
    }

    // Consume coins until the tagret amount has been reached.
    let mut selected_amount = 0;
    let mut selected_coins = Vec::new();

    for coin in coins {
        if selected_amount >= amount {
            break;
        }

        selected_amount += coin.amount;
        selected_coins.push(coin);
    }

    // Not enough value has been accumulated.
    if selected_amount < amount {
        return Err(CoinSelectionError::InsufficientBalance(selected_amount));
    }

    Ok(selected_coins)
}

#[cfg(test)]
mod tests {
    use chia_protocol::Bytes32;

    use super::*;

    // Creates a coin for testing purposes.
    // The index is used to generate a fake parent coin id and puzzle hash.
    fn coin(index: u8, amount: u64) -> Coin {
        Coin::new(
            Bytes32::from([index; 32]),
            Bytes32::from([255 - index; 32]),
            amount,
        )
    }

    #[test]
    fn test_select_coins() {
        let coins = vec![
            coin(0, 100),
            coin(1, 200),
            coin(2, 300),
            coin(3, 400),
            coin(4, 500),
        ];

        // Sorted by amount, ascending.
        let smallest = select_coins(coins.clone(), 700, CoinSelectionMode::Smallest).unwrap();
        assert_eq!(
            smallest,
            vec![coin(0, 100), coin(1, 200), coin(2, 300), coin(3, 400)]
        );

        // Sorted by amount, descending.
        let largest = select_coins(coins, 700, CoinSelectionMode::Largest).unwrap();
        assert_eq!(largest, vec![coin(4, 500), coin(3, 400)]);
    }

    #[test]
    fn test_insufficient_balance() {
        let coins = vec![coin(0, 50), coin(1, 250), coin(2, 100000)];

        // Select an amount that is too high.
        let selected = select_coins(coins, 9999999, CoinSelectionMode::Largest);
        assert_eq!(
            selected,
            Err(CoinSelectionError::InsufficientBalance(100300))
        );
    }

    #[test]
    fn test_no_coins() {
        // There is no amount to select from.
        let selected = select_coins(Vec::new(), 100, CoinSelectionMode::Smallest);
        assert_eq!(selected, Err(CoinSelectionError::NoSpendableCoins));

        // Even if the amount is zero, this should fail.
        let selected = select_coins(Vec::new(), 0, CoinSelectionMode::Largest);
        assert_eq!(selected, Err(CoinSelectionError::NoSpendableCoins));
    }
}
