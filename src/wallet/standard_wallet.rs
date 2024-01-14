use std::sync::Arc;

use chia_protocol::{Coin, CoinSpend};
use chia_wallet::standard::standard_puzzle_hash;
use clvmr::{allocator::NodePtr, Allocator};
use tokio::sync::Mutex;

use crate::{spend_standard_coin, Condition, DerivationState, DerivationWallet, KeyStore, Wallet};

pub struct StandardWallet<S, K> {
    state: S,
    key_store: Arc<Mutex<K>>,
}

impl<S, K> Wallet for StandardWallet<S, K>
where
    S: DerivationState,
    K: KeyStore,
{
    async fn spendable_coins(&self) -> Vec<Coin> {
        self.state.spendable_coins().await
    }

    async fn pending_coins(&self) -> Vec<Coin> {
        self.state.pending_coins().await
    }
}

impl<S, K> DerivationWallet<S, K> for StandardWallet<S, K>
where
    S: DerivationState,
    K: KeyStore,
{
    fn calculate_puzzle_hash(&self, public_key: &chia_bls::PublicKey) -> [u8; 32] {
        standard_puzzle_hash(public_key)
    }

    fn state(&self) -> &S {
        &self.state
    }

    fn state_mut(&mut self) -> &mut S {
        &mut self.state
    }

    fn key_store(&self) -> &Arc<Mutex<K>> {
        &self.key_store
    }
}

impl<S, K> StandardWallet<S, K>
where
    S: DerivationState,
    K: KeyStore,
{
    pub fn new(state: S, key_store: Arc<Mutex<K>>) -> Self {
        Self { state, key_store }
    }

    pub async fn spend_coins(
        &self,
        a: &mut Allocator,
        standard_puzzle_ptr: NodePtr,
        coins: Vec<Coin>,
        conditions: &[Condition<NodePtr>],
    ) -> Vec<CoinSpend> {
        let mut coin_spends = Vec::new();
        for (i, coin) in coins.into_iter().enumerate() {
            let puzzle_hash = &coin.puzzle_hash;
            let index = self
                .state
                .derivation_index(puzzle_hash.into())
                .await
                .expect("cannot spend coin with unknown puzzle hash");

            let synthetic_key = self.key_store.lock().await.public_key(index);

            coin_spends.push(
                spend_standard_coin(
                    a,
                    standard_puzzle_ptr,
                    coin,
                    synthetic_key,
                    if i == 0 { conditions } else { &[] },
                )
                .unwrap(),
            );
        }
        coin_spends
    }
}
