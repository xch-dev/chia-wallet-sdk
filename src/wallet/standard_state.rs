use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinState};
use indexmap::IndexMap;
use itertools::Itertools;

use crate::DerivationInfo;

pub trait StandardState: Send + Sync {
    fn insert_next_derivations(&mut self, derivations: impl IntoIterator<Item = DerivationInfo>);
    fn derivation_index(&self, puzzle_hash: [u8; 32]) -> Option<u32>;
    fn unused_derivation_index(&self) -> Option<u32>;
    fn next_derivation_index(&self) -> u32;
    fn spendable_coins(&self) -> Vec<Coin>;
    fn apply_state_updates(&mut self, updates: Vec<CoinState>);
}

struct DerivationState {
    synthetic_pk: PublicKey,
    coin_states: Vec<CoinState>,
}

#[derive(Default)]
pub struct MemoryStandardState {
    derivations: IndexMap<[u8; 32], DerivationState>,
}

impl MemoryStandardState {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StandardState for MemoryStandardState {
    fn insert_next_derivations(&mut self, derivations: impl IntoIterator<Item = DerivationInfo>) {
        for derivation in derivations {
            self.derivations.insert(
                derivation.puzzle_hash,
                DerivationState {
                    synthetic_pk: derivation.synthetic_pk,
                    coin_states: Vec::new(),
                },
            );
        }
    }

    fn derivation_index(&self, puzzle_hash: [u8; 32]) -> Option<u32> {
        self.derivations
            .get_index_of(&puzzle_hash)
            .map(|index| index as u32)
    }

    fn unused_derivation_index(&self) -> Option<u32> {
        let mut result = None;
        for (i, derivation) in self.derivations.values().enumerate().rev() {
            if derivation.coin_states.is_empty() {
                result = Some(i as u32);
            } else {
                break;
            }
        }
        result
    }

    fn next_derivation_index(&self) -> u32 {
        self.derivations.len() as u32
    }

    fn spendable_coins(&self) -> Vec<Coin> {
        self.derivations
            .values()
            .flat_map(|derivation| &derivation.coin_states)
            .filter(|item| item.created_height.is_some() && item.spent_height.is_none())
            .map(|coin_state| coin_state.coin.clone())
            .collect_vec()
    }

    fn apply_state_updates(&mut self, updates: Vec<CoinState>) {
        for coin_state in updates {
            let puzzle_hash = &coin_state.coin.puzzle_hash;

            if let Some(derivation) = self.derivations.get_mut(<&[u8; 32]>::from(puzzle_hash)) {
                match derivation
                    .coin_states
                    .iter_mut()
                    .find(|item| item.coin == coin_state.coin)
                {
                    Some(value) => *value = coin_state,
                    None => derivation.coin_states.push(coin_state),
                }
            }
        }
    }
}
