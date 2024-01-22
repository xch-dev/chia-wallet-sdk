use std::future::Future;

use chia_protocol::Coin;

mod coin_selection;
mod coin_store;
mod derivation_store;
mod sync;

pub use coin_selection::*;
pub use coin_store::*;
pub use derivation_store::*;
pub use sync::*;

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
