use std::sync::Arc;

use anyhow::Result;
use chia_client::{Peer, PeerEvent};
use chia_protocol::{Coin, CoinSpend, Program, RegisterForPhUpdates, RespondToPhUpdates};
use chia_wallet::standard::{
    standard_puzzle_hash, StandardArgs, StandardSolution, STANDARD_PUZZLE,
};
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvm_utils::CurriedProgram;
use clvmr::{allocator::NodePtr, serde::node_from_bytes, Allocator};
use parking_lot::Mutex;
use tokio::task::JoinHandle;

use crate::{CoinSelectionError, CoinSelectionMode, Condition, DerivationState, KeyStore, Wallet};

pub struct StandardWallet<K, S>
where
    K: KeyStore,
    S: DerivationState,
{
    key_store: Arc<Mutex<K>>,
    peer: Arc<Peer>,
    state: Arc<Mutex<S>>,
    join_handle: Option<JoinHandle<()>>,
}

impl<K, S> StandardWallet<K, S>
where
    K: KeyStore + 'static,
    S: DerivationState + 'static,
{
    pub fn new(key_store: Arc<Mutex<K>>, peer: Arc<Peer>, state: S, gap: u32) -> Self {
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
                    wallet.state.lock().apply_state_updates(update.items);
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

    async fn register_puzzle_hashes(&self, puzzle_hashes: u32) -> Result<Vec<[u8; 32]>> {
        let next = self.next_derivation_index();
        let target = next + puzzle_hashes;
        self.key_store.lock().derive_keys_until(target);

        let derivations = (next..target).map(|index| {
            let public_key = self.key_store.lock().public_key(index);
            standard_puzzle_hash(&public_key)
        });

        self.state
            .lock()
            .insert_next_derivations(derivations.clone());

        let response: RespondToPhUpdates = self
            .peer
            .request(RegisterForPhUpdates::new(
                derivations.map(|derivation| derivation.into()).collect(),
                0,
            ))
            .await?;

        self.state.lock().apply_state_updates(response.coin_states);

        Ok(response
            .puzzle_hashes
            .into_iter()
            .map(|puzzle_hash| (&puzzle_hash).into())
            .collect())
    }

    async fn sync(&self, gap: u32) -> Result<u32> {
        // If there aren't any derivations, generate the first batch.
        if self.next_derivation_index() == 0 {
            self.register_puzzle_hashes(gap).await?;
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
                        self.register_puzzle_hashes(gap).await?;
                    }

                    // Return the unused derivation index.
                    return Ok(unused_index);
                }
                // Otherwise, generate more puzzle hashes and check again.
                None => {
                    self.register_puzzle_hashes(gap).await?;
                }
            }
        }
    }

    pub fn spend_amount(
        &self,
        amount: u64,
        mode: CoinSelectionMode,
        conditions: &[Condition],
    ) -> Result<Vec<CoinSpend>, CoinSelectionError> {
        let coins = self.select_coins(amount, mode)?;
        Ok(self.spend_coins(coins, conditions))
    }

    pub fn spend_coins(&self, coins: Vec<Coin>, conditions: &[Condition]) -> Vec<CoinSpend> {
        let a = &mut Allocator::new();
        let standard_puzzle = node_from_bytes(a, &STANDARD_PUZZLE).unwrap();

        coins
            .into_iter()
            .enumerate()
            .map(|(i, coin)| {
                self.spend_coin(
                    a,
                    standard_puzzle,
                    coin,
                    if i == 0 { conditions } else { &[] },
                )
            })
            .collect::<Result<_>>()
            .unwrap()
    }

    fn spend_coin(
        &self,
        a: &mut Allocator,
        standard_puzzle: NodePtr,
        coin: Coin,
        conditions: &[Condition],
    ) -> Result<CoinSpend> {
        let puzzle_hash = &coin.puzzle_hash;
        let index = self
            .derivation_index(puzzle_hash.into())
            .expect("cannot spend coin with unknown puzzle hash");
        let synthetic_key = self.key_store.lock().public_key(index);

        let puzzle = CurriedProgram {
            program: standard_puzzle,
            args: StandardArgs { synthetic_key },
        }
        .to_clvm(a)?;

        let solution = StandardSolution {
            original_public_key: None,
            delegated_puzzle: clvm_quote!(conditions).to_clvm(a).unwrap(),
            solution: a.null(),
        }
        .to_clvm(a)?;

        let puzzle = Program::from_clvm(a, puzzle)?;
        let solution = Program::from_clvm(a, solution)?;
        Ok(CoinSpend::new(coin, puzzle, solution))
    }

    fn derivation_index(&self, puzzle_hash: [u8; 32]) -> Option<u32> {
        self.state.lock().derivation_index(puzzle_hash)
    }

    fn unused_derivation_index(&self) -> Option<u32> {
        self.state.lock().unused_derivation_index()
    }

    fn next_derivation_index(&self) -> u32 {
        self.state.lock().next_derivation_index()
    }
}

impl<K, S> Wallet for StandardWallet<K, S>
where
    K: KeyStore,
    S: DerivationState,
{
    fn spendable_coins(&self) -> Vec<Coin> {
        self.state.lock().spendable_coins()
    }
}

impl<K, S> Drop for StandardWallet<K, S>
where
    K: KeyStore,
    S: DerivationState,
{
    fn drop(&mut self) {
        if let Some(join_handle) = self.join_handle.take() {
            join_handle.abort();
        }
    }
}
