use std::sync::Arc;

use async_trait::async_trait;
use chia_client::{Peer, PeerEvent};
use chia_protocol::{Coin, CoinState, RegisterForPhUpdates, RespondToPhUpdates};
use chia_wallet::{
    standard::{standard_puzzle_hash, DEFAULT_HIDDEN_PUZZLE_HASH},
    DeriveSynthetic,
};
use indexmap::IndexMap;
use itertools::Itertools;
use parking_lot::Mutex;
use tokio::task::JoinHandle;

use crate::KeyStore;

pub trait Wallet {
    fn spendable_coins(&self) -> Vec<Coin>;

    fn spendable_balance(&self) -> u64 {
        self.spendable_coins()
            .iter()
            .fold(0, |balance, coin| balance + coin.amount)
    }
}

#[async_trait]
pub trait DerivationWallet {
    fn puzzle_hash(&self, index: u32) -> [u8; 32];
    fn unused_derivation_index(&self) -> Option<u32>;
    fn next_derivation_index(&self) -> u32;

    async fn generate_puzzle_hashes(&self, puzzle_hashes: u32) -> anyhow::Result<Vec<[u8; 32]>>;

    async fn sync(&self, gap: u32) -> anyhow::Result<u32> {
        // If there aren't any derivations, generate the first batch.
        if self.next_derivation_index() == 0 {
            self.generate_puzzle_hashes(gap).await?;
        }

        loop {
            match self.unused_derivation_index() {
                // Check if an unused derivation index was found.
                Some(unused_index) => {
                    // If so, calculate the extra unused derivations after that index.
                    let last_index = self.next_derivation_index() - 1;
                    let extra_indices = last_index - unused_index;

                    // Make sure at least `gap` indices are available if needed.
                    if extra_indices < gap {
                        self.generate_puzzle_hashes(gap).await?;
                    }

                    // Return the unused derivation index.
                    return Ok(unused_index);
                }
                // Otherwise, generate more puzzle hashes and check again.
                None => {
                    self.generate_puzzle_hashes(gap).await?;
                }
            }
        }
    }
}

pub struct StandardWallet {
    key_store: Arc<KeyStore>,
    peer: Arc<Peer>,
    state: Arc<Mutex<StandardState>>,
    join_handle: Option<JoinHandle<()>>,
}

impl Wallet for StandardWallet {
    fn spendable_coins(&self) -> Vec<Coin> {
        self.state.lock().spendable_coins()
    }
}

#[async_trait]
impl DerivationWallet for StandardWallet {
    fn puzzle_hash(&self, index: u32) -> [u8; 32] {
        let public_key = self.key_store.public_key(index);
        let synthetic_key = public_key.derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);
        standard_puzzle_hash(&synthetic_key)
    }

    fn unused_derivation_index(&self) -> Option<u32> {
        self.state.lock().unused_derivation_index()
    }

    fn next_derivation_index(&self) -> u32 {
        self.state.lock().derived_puzzle_hashes.len() as u32
    }

    async fn generate_puzzle_hashes(&self, puzzle_hashes: u32) -> anyhow::Result<Vec<[u8; 32]>> {
        let derivation_index = self.next_derivation_index();

        let puzzle_hashes = (derivation_index..derivation_index + puzzle_hashes)
            .map(|index| self.puzzle_hash(index));

        self.state.lock().add_puzzle_hashes(puzzle_hashes.clone());

        let response: RespondToPhUpdates = self
            .peer
            .request(RegisterForPhUpdates::new(
                puzzle_hashes.map(Into::into).collect(),
                0,
            ))
            .await?;

        self.state.lock().apply_updates(response.coin_states);

        Ok(response
            .puzzle_hashes
            .into_iter()
            .map(|puzzle_hash| (&puzzle_hash).into())
            .collect())
    }
}

impl StandardWallet {
    pub fn new(key_store: Arc<KeyStore>, peer: Arc<Peer>, state: StandardState, gap: u32) -> Self {
        let mut event_receiver = peer.receiver().resubscribe();
        let state = Arc::new(Mutex::new(state));

        let wallet = Self {
            key_store: key_store.clone(),
            peer: peer.clone(),
            state: state.clone(),
            join_handle: None,
        };

        let join_handle = tokio::spawn(async move {
            if let Err(error) = wallet.sync(gap).await {
                log::error!("failed to perform initial wallet sync: {error}");
            }

            while let Ok(event) = event_receiver.recv().await {
                if let PeerEvent::CoinStateUpdate(update) = event {
                    wallet.state.lock().apply_updates(update.items);
                    if let Err(error) = wallet.sync(gap).await {
                        log::error!("failed to sync wallet after coin state update: {error}");
                    }
                }
            }
        });

        Self {
            key_store,
            peer,
            state,
            join_handle: Some(join_handle),
        }
    }
}

impl Drop for StandardWallet {
    fn drop(&mut self) {
        if let Some(join_handle) = self.join_handle.take() {
            join_handle.abort();
        }
    }
}

#[derive(Default)]
pub struct StandardState {
    derived_puzzle_hashes: IndexMap<[u8; 32], Vec<CoinState>>,
}

impl StandardState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_puzzle_hashes(&mut self, puzzle_hashes: impl Iterator<Item = [u8; 32]>) {
        for puzzle_hash in puzzle_hashes {
            self.derived_puzzle_hashes.insert(puzzle_hash, Vec::new());
        }
    }

    pub fn unused_derivation_index(&self) -> Option<u32> {
        let mut result = None;
        for (i, coin_states) in self.derived_puzzle_hashes.values().enumerate().rev() {
            if coin_states.is_empty() {
                result = Some(i as u32);
            } else {
                break;
            }
        }
        result
    }

    pub fn spendable_coins(&self) -> Vec<Coin> {
        self.derived_puzzle_hashes
            .values()
            .flatten()
            .filter(|item| item.created_height.is_some() && item.spent_height.is_none())
            .map(|coin_state| coin_state.coin.clone())
            .collect_vec()
    }

    pub fn apply_updates(&mut self, updates: Vec<CoinState>) {
        for coin_state in updates {
            let puzzle_hash = &coin_state.coin.puzzle_hash;

            if let Some(coin_states) = self
                .derived_puzzle_hashes
                .get_mut(<&[u8; 32]>::from(puzzle_hash))
            {
                match coin_states
                    .iter_mut()
                    .find(|item| item.coin == coin_state.coin)
                {
                    Some(value) => *value = coin_state,
                    None => coin_states.push(coin_state),
                }
            }
        }
    }
}
