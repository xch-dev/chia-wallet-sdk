use std::sync::Arc;

use chia_client::Peer;
use chia_protocol::CoinState;
use itertools::Itertools;

pub trait StandardState {
    fn spendable_coins(&self) -> Vec<CoinState>;
}

pub struct InMemoryStandardState {
    peer: Arc<Peer>,
    standard_coins: Vec<CoinState>,
}

impl InMemoryStandardState {
    pub fn new(peer: Arc<Peer>) -> Self {
        Self {
            peer,
            standard_coins: Vec::new(),
        }
    }
}

impl StandardState for InMemoryStandardState {
    fn spendable_coins(&self) -> Vec<CoinState> {
        self.standard_coins
            .iter()
            .filter(|item| item.created_height.is_some() && item.spent_height.is_none())
            .cloned()
            .collect_vec()
    }
}
