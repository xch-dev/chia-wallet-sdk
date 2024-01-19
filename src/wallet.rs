use std::future::Future;

use chia_protocol::Coin;

mod cat_wallet;
mod derivation_state;
mod derivation_wallet;
mod standard_wallet;

pub use cat_wallet::*;
pub use derivation_state::*;
pub use derivation_wallet::*;
pub use standard_wallet::*;

/// A wallet is responsible for managing coins.
pub trait Wallet: Sync {
    /// Returns the coins that are spendable by this wallet.
    fn spendable_coins(&self) -> impl Future<Output = Vec<Coin>> + Send;

    /// Returns the total amount of coins that are spendable by this wallet.
    fn spendable_balance(&self) -> impl Future<Output = u64> + Send {
        async {
            self.spendable_coins()
                .await
                .iter()
                .fold(0, |balance, coin| balance + coin.amount)
        }
    }

    /// Returns the coins that are either spendable or pending in this wallet.
    fn pending_coins(&self) -> impl Future<Output = Vec<Coin>> + Send;

    /// Returns the total amount of coins that are either spendable or pending in this wallet.
    fn pending_balance(&self) -> impl Future<Output = u64> + Send {
        async {
            self.pending_coins()
                .await
                .iter()
                .fold(0, |balance, coin| balance + coin.amount)
        }
    }
}
