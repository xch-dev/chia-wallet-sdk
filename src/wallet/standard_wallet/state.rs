use chia_protocol::CoinState;
use itertools::Itertools;

pub trait StandardState: Send + Sync {
    fn spendable_coins(&self) -> Vec<CoinState>;
    fn update_coin_states(&mut self, updates: Vec<CoinState>);
}

pub struct InMemoryStandardState {
    coin_states: Vec<CoinState>,
}

impl InMemoryStandardState {
    pub fn new() -> Self {
        Self {
            coin_states: Vec::new(),
        }
    }
}

impl StandardState for InMemoryStandardState {
    fn spendable_coins(&self) -> Vec<CoinState> {
        self.coin_states
            .iter()
            .filter(|item| item.created_height.is_some() && item.spent_height.is_none())
            .cloned()
            .collect_vec()
    }

    fn update_coin_states(&mut self, updates: Vec<CoinState>) {
        for coin_state in updates {
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
