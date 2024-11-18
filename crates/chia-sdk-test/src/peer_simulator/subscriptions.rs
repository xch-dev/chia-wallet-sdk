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

    pub(crate) fn remove_coin_subscriptions(
        &mut self,
        peer: SocketAddr,
        coin_ids: &[Bytes32],
    ) -> Vec<Bytes32> {
        let mut removed = Vec::new();

        if let Some(subscriptions) = self.coin_subscriptions.get_mut(&peer) {
            for coin_id in coin_ids {
                if subscriptions.swap_remove(coin_id) {
                    removed.push(*coin_id);
                }
            }
            if subscriptions.is_empty() {
                self.coin_subscriptions.swap_remove(&peer);
            }
        }

        removed
    }

    pub(crate) fn remove_puzzle_subscriptions(
        &mut self,
        peer: SocketAddr,
        puzzle_hashes: &[Bytes32],
    ) -> Vec<Bytes32> {
        let mut removed = Vec::new();

        if let Some(subscriptions) = self.puzzle_subscriptions.get_mut(&peer) {
            for puzzle_hash in puzzle_hashes {
                if subscriptions.swap_remove(puzzle_hash) {
                    removed.push(*puzzle_hash);
                }
            }
            if subscriptions.is_empty() {
                self.puzzle_subscriptions.swap_remove(&peer);
            }
        }

        removed
    }

    pub(crate) fn remove_all_coin_subscriptions(&mut self, peer: SocketAddr) -> Vec<Bytes32> {
        self.coin_subscriptions
            .swap_remove(&peer)
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    pub(crate) fn remove_all_puzzle_subscriptions(&mut self, peer: SocketAddr) -> Vec<Bytes32> {
        self.puzzle_subscriptions
            .swap_remove(&peer)
            .unwrap_or_default()
            .into_iter()
            .collect()
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
