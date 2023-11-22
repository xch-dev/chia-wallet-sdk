use async_trait::async_trait;
use chia_protocol::Coin;

mod cat_wallet;
mod derivation_state;
mod derivation_wallet;
mod standard_wallet;

pub use cat_wallet::*;
pub use derivation_state::*;
pub use derivation_wallet::*;
pub use standard_wallet::*;

#[async_trait]
pub trait Wallet {
    /// Returns the coins that are spendable by this wallet.
    async fn spendable_coins(&self) -> Vec<Coin>;

    /// Returns the total amount of coins that are spendable by this wallet.
    async fn spendable_balance(&self) -> u64 {
        self.spendable_coins()
            .await
            .iter()
            .fold(0, |balance, coin| balance + coin.amount)
    }

    /// Returns the coins that are either spendable or pending in this wallet.
    async fn pending_coins(&self) -> Vec<Coin>;

    /// Returns the total amount of coins that are either spendable or pending in this wallet.
    async fn pending_balance(&self) -> u64 {
        self.pending_coins()
            .await
            .iter()
            .fold(0, |balance, coin| balance + coin.amount)
    }
}
