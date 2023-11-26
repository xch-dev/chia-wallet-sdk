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
    async fn pending_coins(&self) -> Vec<Coin>;
    async fn coin_state(&self, coin: &Coin) -> Option<CoinState>;
    async fn apply_state_updates(&mut self, updates: Vec<CoinState>);
}

#[derive(Default)]
pub struct MemoryDerivationState {
    derivations: IndexMap<[u8; 32], Vec<CoinState>>,
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
            .filter(|item| item.created_height.is_some() && item.spent_height.is_none())
            .map(|coin_state| coin_state.coin.clone())
            .collect()
    }

    async fn pending_coins(&self) -> Vec<Coin> {
        self.derivations
            .values()
            .flatten()
            .filter(|item| item.spent_height.is_none())
            .map(|coin_state| coin_state.coin.clone())
            .collect()
    }

    async fn coin_state(&self, coin: &Coin) -> Option<CoinState> {
        let puzzle_hash: [u8; 32] = (&coin.puzzle_hash).into();
        self.derivations
            .get(&puzzle_hash)
            .and_then(|coin_states| coin_states.iter().find(|item| item.coin == *coin).cloned())
    }

    async fn apply_state_updates(&mut self, updates: Vec<CoinState>) {
        for coin_state in updates {
            let puzzle_hash = &coin_state.coin.puzzle_hash;

            if let Some(derivation) = self.derivations.get_mut(<&[u8; 32]>::from(puzzle_hash)) {
                match derivation
                    .iter_mut()
                    .find(|item| item.coin == coin_state.coin)
                {
                    Some(value) => *value = coin_state,
                    None => derivation.push(coin_state),
                }
            }
        }
    }
}
