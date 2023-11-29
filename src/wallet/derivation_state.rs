use async_trait::async_trait;
use chia_protocol::{Coin, CoinState};
use indexmap::IndexMap;

#[async_trait]
pub trait DerivationState: Send + Sync {
    async fn insert_next_derivations(&mut self, derivations: Vec<[u8; 32]>);
    async fn derivation_index(&self, puzzle_hash: [u8; 32]) -> Option<u32>;
    async fn unused_derivation_index(&self) -> Option<u32>;
    async fn next_derivation_index(&self) -> u32;
    async fn spendable_coins(&self) -> Vec<Coin>;
    async fn unconfirmed_coins(&self) -> Vec<Coin>;
    async fn coin_state(&self, coin_id: [u8; 32]) -> Option<CoinState>;
    async fn apply_state_updates(&mut self, updates: Vec<CoinState>);
    async fn is_pending(&self, coin_id: [u8; 32]) -> bool;
    async fn set_pending(&mut self, coin_id: [u8; 32], is_pending: bool);
    async fn pending_coins(&self) -> Vec<Coin>;
}

struct CoinData {
    state: CoinState,
    is_pending: bool,
}

#[derive(Default)]
pub struct MemoryDerivationState {
    derivations: IndexMap<[u8; 32], Vec<CoinData>>,
}

impl MemoryDerivationState {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl DerivationState for MemoryDerivationState {
    async fn insert_next_derivations(&mut self, derivations: Vec<[u8; 32]>) {
        for derivation in derivations {
            self.derivations.insert(derivation, Vec::new());
        }
    }

    async fn derivation_index(&self, puzzle_hash: [u8; 32]) -> Option<u32> {
        self.derivations
            .get_index_of(&puzzle_hash)
            .map(|index| index as u32)
    }

    async fn unused_derivation_index(&self) -> Option<u32> {
        let mut result = None;
        for (i, derivation) in self.derivations.values().enumerate().rev() {
            if derivation.is_empty() {
                result = Some(i as u32);
            } else {
                break;
            }
        }
        result
    }

    async fn next_derivation_index(&self) -> u32 {
        self.derivations.len() as u32
    }

    async fn spendable_coins(&self) -> Vec<Coin> {
        self.derivations
            .values()
            .flatten()
            .filter(|item| {
                item.state.created_height.is_some()
                    && item.state.spent_height.is_none()
                    && !item.is_pending
            })
            .map(|coin_state| coin_state.state.coin.clone())
            .collect()
    }

    async fn unconfirmed_coins(&self) -> Vec<Coin> {
        self.derivations
            .values()
            .flatten()
            .filter(|item| item.state.spent_height.is_none() && !item.is_pending)
            .map(|coin_state| coin_state.state.coin.clone())
            .collect()
    }

    async fn coin_state(&self, coin_id: [u8; 32]) -> Option<CoinState> {
        self.derivations
            .values()
            .flatten()
            .find(|item| item.state.coin.coin_id() == coin_id)
            .map(|item| item.state.clone())
    }

    async fn apply_state_updates(&mut self, updates: Vec<CoinState>) {
        for coin_state in updates {
            let puzzle_hash = &coin_state.coin.puzzle_hash;
            let data = CoinData {
                state: coin_state.clone(),
                is_pending: false,
            };

            if let Some(derivation) = self.derivations.get_mut(<&[u8; 32]>::from(puzzle_hash)) {
                match derivation
                    .iter_mut()
                    .find(|item| item.state.coin == coin_state.coin)
                {
                    Some(value) => {
                        *value = data;
                    }
                    None => derivation.push(data),
                }
            }
        }
    }

    async fn is_pending(&self, coin_id: [u8; 32]) -> bool {
        self.derivations
            .values()
            .flatten()
            .find(|item| item.state.coin.coin_id() == coin_id)
            .is_some_and(|item| item.is_pending)
    }

    async fn set_pending(&mut self, coin_id: [u8; 32], is_pending: bool) {
        if let Some(item) = self
            .derivations
            .values_mut()
            .flatten()
            .find(|item| item.state.coin.coin_id() == coin_id)
        {
            item.is_pending = is_pending;
        }
    }

    async fn pending_coins(&self) -> Vec<Coin> {
        self.derivations
            .values()
            .flatten()
            .filter(|item| item.is_pending)
            .map(|coin_state| coin_state.state.coin.clone())
            .collect()
    }
}
