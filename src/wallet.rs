mod standard_wallet;

use chia_protocol::Coin;
pub use standard_wallet::*;

use crate::{select_coins, CoinSelectionError, CoinSelectionMode};

pub trait Wallet {
    fn spendable_coins(&self) -> Vec<Coin>;

    fn select_coins(
        &self,
        amount: u64,
        mode: CoinSelectionMode,
    ) -> Result<Vec<Coin>, CoinSelectionError> {
        select_coins(self.spendable_coins(), amount, mode)
    }

    fn spendable_balance(&self) -> u64 {
        self.spendable_coins()
            .iter()
            .fold(0, |balance, coin| balance + coin.amount)
    }
}
