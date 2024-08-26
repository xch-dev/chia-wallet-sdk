use std::net::SocketAddr;

use chia_protocol::Bytes32;
use indexmap::{IndexMap, IndexSet};

#[derive(Debug, Default, Clone)]
pub(crate) struct Subscriptions {
    puzzle_subscriptions: IndexMap<SocketAddr, IndexSet<Bytes32>>,
    coin_subscriptions: IndexMap<SocketAddr, IndexSet<Bytes32>>,
}

impl Subscriptions {
    pub(crate) fn add_coin_subscriptions(&mut self, peer: SocketAddr, coin_ids: IndexSet<Bytes32>) {
        self.coin_subscriptions
            .entry(peer)
            .or_default()
            .extend(coin_ids);
    }

    pub(crate) fn add_puzzle_subscriptions(
        &mut self,
        peer: SocketAddr,
        puzzle_hashes: IndexSet<Bytes32>,
    ) {
        self.puzzle_subscriptions
            .entry(peer)
            .or_default()
            .extend(puzzle_hashes);
    }

    pub(crate) fn subscription_count(&self, peer: SocketAddr) -> usize {
        self.coin_subscriptions.get(&peer).map_or(0, IndexSet::len)
            + self
                .puzzle_subscriptions
                .get(&peer)
                .map_or(0, IndexSet::len)
    }

    pub(crate) fn peers(&self) -> IndexSet<SocketAddr> {
        self.coin_subscriptions
            .keys()
            .chain(self.puzzle_subscriptions.keys())
            .copied()
            .collect()
    }

    pub(crate) fn coin_subscriptions(&self, peer: SocketAddr) -> Option<&IndexSet<Bytes32>> {
        self.coin_subscriptions.get(&peer)
    }

    pub(crate) fn puzzle_subscriptions(&self, peer: SocketAddr) -> Option<&IndexSet<Bytes32>> {
        self.puzzle_subscriptions.get(&peer)
    }
}
