use std::sync::Arc;

use chia_client::Peer;
use chia_protocol::CoinState;

pub trait StandardState {}

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

impl StandardState for InMemoryStandardState {}
