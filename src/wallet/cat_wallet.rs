use std::sync::Arc;

use chia_bls::PublicKey;
use chia_client::Peer;
use chia_protocol::{Coin, CoinSpend, RegisterForCoinUpdates, RespondToCoinUpdates};
use chia_wallet::{
    cat::{CatArgs, CAT_PUZZLE, CAT_PUZZLE_HASH},
    standard::{standard_puzzle_hash, STANDARD_PUZZLE},
    LineageProof,
};
use clvm_utils::tree_hash;
use clvmr::{allocator::NodePtr, serde::node_from_bytes, Allocator};
use tokio::sync::Mutex;

use crate::{
    cat_puzzle_hash, spend_cat_coins, utils::request_puzzle_args, CatCondition, CatSpend,
    DerivationState, DerivationWallet, KeyStore, Wallet,
};

/// A wallet that can spend CAT coins.
pub struct CatWallet<S, K> {
    asset_id: [u8; 32],
    state: S,
    key_store: Arc<Mutex<K>>,
}

impl<S, K> Wallet for CatWallet<S, K>
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

impl<S, K> DerivationWallet<S, K> for CatWallet<S, K>
where
    S: DerivationState + 'static,
    K: KeyStore,
{
    fn calculate_puzzle_hash(&self, public_key: &PublicKey) -> [u8; 32] {
        let inner_puzzle_hash = standard_puzzle_hash(public_key);
        cat_puzzle_hash(self.asset_id, inner_puzzle_hash)
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

impl<S, K> CatWallet<S, K>
where
    S: DerivationState + 'static,
    K: KeyStore,
{
    /// Creates a new CAT wallet with a given asset id.
    pub fn new(state: S, key_store: Arc<Mutex<K>>, asset_id: [u8; 32]) -> Self {
        Self {
            state,
            key_store,
            asset_id,
        }
    }

    /// Creates a CAT spend.
    pub async fn spend_coins(
        &self,
        peer: &Peer,
        coins: Vec<Coin>,
        conditions: Vec<CatCondition<NodePtr>>,
    ) -> Vec<CoinSpend> {
        let mut a = Allocator::new();
        let standard_puzzle = node_from_bytes(&mut a, &STANDARD_PUZZLE).unwrap();
        let cat_puzzle = node_from_bytes(&mut a, &CAT_PUZZLE).unwrap();

        let mut spends = Vec::new();
        let mut conditions = Some(conditions);

        let parent_coin_updates: RespondToCoinUpdates = peer
            .request(RegisterForCoinUpdates::new(
                coins.iter().map(|coin| coin.parent_coin_info).collect(),
                0,
            ))
            .await
            .unwrap();

        for (i, coin) in coins.into_iter().enumerate() {
            // Coin info.
            let puzzle_hash = &coin.puzzle_hash;
            let index = self
                .state
                .derivation_index(puzzle_hash.into())
                .await
                .expect("cannot spend coin with unknown puzzle hash");

            let synthetic_key = self.key_store().lock().await.public_key(index).await;
            let p2_puzzle_hash = standard_puzzle_hash(&synthetic_key);

            // Lineage proof.
            let parent_coin_state = parent_coin_updates
                .coin_states
                .iter()
                .find(|coin_state| coin_state.coin.coin_id() == coin.parent_coin_info)
                .cloned()
                .unwrap();

            let cat_args: CatArgs<NodePtr> = request_puzzle_args(
                &mut a,
                peer,
                &coin,
                CAT_PUZZLE_HASH,
                parent_coin_state.spent_height.unwrap(),
            )
            .await
            .unwrap();

            // Spend information.
            let spend = CatSpend {
                coin,
                synthetic_key,
                conditions: if i == 0 {
                    conditions.take().unwrap()
                } else {
                    Vec::new()
                },
                extra_delta: 0,
                p2_puzzle_hash,
                lineage_proof: LineageProof {
                    parent_coin_info: parent_coin_state.coin.parent_coin_info,
                    inner_puzzle_hash: tree_hash(&a, cat_args.inner_puzzle).into(),
                    amount: parent_coin_state.coin.amount,
                },
            };
            spends.push(spend);
        }

        spend_cat_coins(&mut a, standard_puzzle, cat_puzzle, &self.asset_id, &spends).unwrap()
    }
}
