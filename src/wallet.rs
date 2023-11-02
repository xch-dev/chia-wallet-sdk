mod standard_wallet;

use chia_protocol::CoinState;
pub use standard_wallet::*;

pub trait Wallet {
    fn spendable_coins(&self) -> Vec<CoinState>;

    fn spendable_balance(&self) -> u64 {
        self.spendable_coins()
            .iter()
            .fold(0, |balance, coin_state| balance + coin_state.coin.amount)
    }
}
