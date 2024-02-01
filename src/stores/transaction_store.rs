use std::future::Future;

use chia_protocol::{Coin, SpendBundle};

/// Keeps track of pending transactions for a wallet.
pub trait TransactionStore {
    /// Gets a list of coins that are currently pending a spend.
    fn spent_coins(&self) -> impl Future<Output = Vec<Coin>> + Send;

    /// Gets a list of transaction ids.
    fn transactions(&self) -> impl Future<Output = Vec<[u8; 32]>> + Send;

    /// Gets a pending transaction by its id.
    fn transaction(
        &self,
        transaction_id: [u8; 32],
    ) -> impl Future<Output = Option<SpendBundle>> + Send;

    /// Gets the spent coin ids for a given transaction.
    fn removals(&self, transaction_id: [u8; 32]) -> impl Future<Output = Vec<Coin>> + Send;

    /// Adds a transaction to the store.
    fn add_transaction(&self, spend_bundle: SpendBundle) -> impl Future<Output = bool> + Send;

    /// Removes a transaction from the store.
    fn remove_transaction(&self, transaction_id: [u8; 32]) -> impl Future<Output = bool> + Send;
}
