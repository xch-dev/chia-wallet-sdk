use std::collections::HashSet;

use chia_protocol::{Coin, CoinState};
use itertools::Itertools;

pub trait StandardState: Send + Sync {
    /// Looks up created but not spent or pending coins.
    fn spendable_coins(&self) -> Vec<Coin>;

    /// Marks a coin as pending being spent.
    fn mark_pending(&mut self, coin_id: [u8; 32]);

    /// Unmarks a coin as pending being spent.
    fn unmark_pending(&mut self, coin_id: [u8; 32]);

    /// Applies coin state updates.
    fn update_coin_states(&mut self, updates: Vec<CoinState>);
}

#[derive(Default)]
pub struct InMemoryStandardState {
    coin_states: Vec<CoinState>,
    pending_spent: HashSet<[u8; 32]>,
}

impl InMemoryStandardState {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StandardState for InMemoryStandardState {
    fn spendable_coins(&self) -> Vec<Coin> {
        self.coin_states
            .iter()
            .filter(|item| {
                item.created_height.is_some()
                    && item.spent_height.is_none()
                    && !self.pending_spent.contains(&item.coin.coin_id())
            })
            .map(|coin_state| coin_state.coin.clone())
            .collect_vec()
    }

    fn mark_pending(&mut self, coin_id: [u8; 32]) {
        self.pending_spent.insert(coin_id);
    }

    fn unmark_pending(&mut self, coin_id: [u8; 32]) {
        self.pending_spent.remove(&coin_id);
    }

    fn update_coin_states(&mut self, updates: Vec<CoinState>) {
        for coin_state in updates {
            // Remove from pending if spent.
            if coin_state.spent_height.is_some() {
                self.pending_spent.remove(&coin_state.coin.coin_id());
            }

            // Upsert coin state.
            match self
                .coin_states
                .iter_mut()
                .find(|item| item.coin == coin_state.coin)
            {
                Some(value) => *value = coin_state,
                None => self.coin_states.push(coin_state),
            }
        }
    }
}
