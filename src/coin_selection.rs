use chia_protocol::Coin;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoinSelectionMode {
    Largest,
    Smallest,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CoinSelectionError {
    #[error("no spendable coins")]
    NoSpendableCoins,

    #[error("insufficient balance {0}")]
    InsufficientBalance(u64),
}

pub fn select_coins(
    mut coins: Vec<Coin>,
    amount: u64,
    mode: CoinSelectionMode,
) -> Result<Vec<Coin>, CoinSelectionError> {
    if coins.is_empty() {
        return Err(CoinSelectionError::NoSpendableCoins);
    }

    match mode {
        CoinSelectionMode::Largest => {
            coins.sort_by(|a, b| b.amount.cmp(&a.amount));
        }
        CoinSelectionMode::Smallest => {
            coins.sort_by(|a, b| a.amount.cmp(&b.amount));
        }
    }

    let mut selected_amount = 0;
    let mut selected_coins = Vec::new();

    for coin in coins {
        if selected_amount >= amount {
            break;
        }

        selected_amount += coin.amount;
        selected_coins.push(coin);
    }

    if selected_amount < amount {
        return Err(CoinSelectionError::InsufficientBalance(selected_amount));
    }

    Ok(selected_coins)
}
