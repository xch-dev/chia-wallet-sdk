//! Changes that a [`PeerSimulator`](crate::PeerSimulator) serving this simulator announces to
//! connected peers. Nothing is recorded unless the peer simulator enables the log.

#![cfg_attr(not(feature = "peer-simulator"), allow(dead_code))]

use chia_protocol::{Bytes32, CoinState, MempoolRemoveReason};
use indexmap::{IndexMap, IndexSet};

use super::{FullNodeSimulator, ValidatedBundle};

#[derive(Debug, Clone, Default)]
pub(crate) struct PeerEventLog {
    enabled: bool,
    events: Vec<PeerEvent>,
    /// The ids of each mempool item, as of when it was added. Removals and replays use these,
    /// so an item is removed for the same peers it was added for, even if a coin it touched (and
    /// so its hint) is gone by the time it's removed.
    item_ids: IndexMap<Bytes32, MempoolItemIds>,
}

/// A change to the chain or the mempool, in the order it happened.
#[derive(Debug, Clone)]
pub enum PeerEvent {
    /// A block was farmed. The coin states are the coins it created or spent, as of that block,
    /// along with their hints.
    Block {
        height: u32,
        header_hash: Bytes32,
        coin_states: Vec<(CoinState, Vec<Bytes32>)>,
    },
    /// A transaction entered the mempool.
    MempoolItemAdded {
        transaction_id: Bytes32,
        ids: MempoolItemIds,
    },
    /// A transaction left the mempool.
    MempoolItemRemoved {
        transaction_id: Bytes32,
        reason: MempoolRemoveReason,
        ids: MempoolItemIds,
    },
}

/// The ids a mempool item touches, which CHIP-26 matches against a peer's subscriptions.
#[derive(Debug, Clone, Default)]
pub struct MempoolItemIds {
    /// The ids of the coins the item spends and creates.
    pub coin_ids: IndexSet<Bytes32>,
    /// The puzzle hashes and hints of the coins the item spends and creates.
    pub puzzle_hashes: IndexSet<Bytes32>,
}

impl FullNodeSimulator {
    fn mempool_item_ids(&self, item: &ValidatedBundle) -> MempoolItemIds {
        let mut ids = MempoolItemIds::default();
        for (coin_id, spend) in &item.spends {
            ids.coin_ids.insert(*coin_id);
            ids.puzzle_hashes.insert(spend.coin_spend.coin.puzzle_hash);
            ids.puzzle_hashes.extend(self.spent_coin_hint(*coin_id));
        }
        for (coin, hint) in &item.additions {
            ids.coin_ids.insert(coin.coin_id());
            ids.puzzle_hashes.insert(coin.puzzle_hash);
            ids.puzzle_hashes.extend(*hint);
        }
        ids
    }

    /// The hint of a coin that is spent, whether it's in the coin set or created by a mempool
    /// item.
    fn spent_coin_hint(&self, coin_id: Bytes32) -> Option<Bytes32> {
        if let Some(hint) = self.state.coin_hints.get(&coin_id) {
            return Some(*hint);
        }
        self.mempool
            .values()
            .flat_map(|item| &item.additions)
            .find(|(coin, _)| coin.coin_id() == coin_id)
            .and_then(|(_, hint)| *hint)
    }

    pub(super) fn record_block_for_peers(
        &mut self,
        height: u32,
        header_hash: Bytes32,
        coin_ids: &IndexSet<Bytes32>,
    ) {
        if !self.peer_events.enabled {
            return;
        }
        let coin_states = coin_ids
            .iter()
            .filter_map(|coin_id| {
                let record = self.state.coins.get(coin_id)?;
                Some((
                    record.to_coin_state(),
                    self.state
                        .coin_hints
                        .get(coin_id)
                        .copied()
                        .into_iter()
                        .collect(),
                ))
            })
            .collect();
        self.peer_events.events.push(PeerEvent::Block {
            height,
            header_hash,
            coin_states,
        });
    }

    pub(super) fn record_mempool_added(&mut self, transaction_id: Bytes32) {
        if !self.peer_events.enabled {
            return;
        }
        let Some(item) = self.mempool.get(&transaction_id) else {
            return;
        };
        let ids = self.mempool_item_ids(item);
        self.peer_events
            .item_ids
            .insert(transaction_id, ids.clone());
        self.peer_events.events.push(PeerEvent::MempoolItemAdded {
            transaction_id,
            ids,
        });
    }

    pub(super) fn record_mempool_removed(
        &mut self,
        transaction_id: Bytes32,
        item: &ValidatedBundle,
        reason: MempoolRemoveReason,
    ) {
        if !self.peer_events.enabled {
            return;
        }
        let ids = self
            .peer_events
            .item_ids
            .shift_remove(&transaction_id)
            .unwrap_or_else(|| self.mempool_item_ids(item));
        self.peer_events.events.push(PeerEvent::MempoolItemRemoved {
            transaction_id,
            reason,
            ids,
        });
    }

    /// Stops recording peer events, and returns whether they were being recorded.
    pub(super) fn pause_peer_events(&mut self) -> bool {
        std::mem::replace(&mut self.peer_events.enabled, false)
    }

    pub(super) fn resume_peer_events(&mut self, enabled: bool) {
        self.peer_events.enabled = enabled;
    }
}

#[cfg(feature = "peer-simulator")]
mod backend {
    use chia_protocol::{Bytes32, CoinState, Program, SpendBundle};
    use indexmap::IndexSet;

    use crate::{FullNodeSimulator, PeerSimulatorConfig, SimulatorError, peer_simulator::Backend};

    use super::{MempoolItemIds, PeerEvent};

    impl Backend for FullNodeSimulator {
        const HAS_MEMPOOL: bool = true;

        fn new_for_peers(config: &PeerSimulatorConfig) -> Self {
            let mut simulator = Self::new();
            simulator.set_genesis_challenge(config.genesis_challenge);
            simulator.peer_events.enabled = true;
            simulator
        }

        fn peak_height(&self) -> u32 {
            self.height()
        }

        fn peak_header_hash(&self) -> Bytes32 {
            self.header_hash()
        }

        fn header_hash_at(&self, height: u32) -> Option<Bytes32> {
            self.header_hash_of(height)
        }

        fn coin_states(&self, coin_ids: &IndexSet<Bytes32>) -> Vec<CoinState> {
            coin_ids
                .iter()
                .filter_map(|coin_id| self.state.coins.get(coin_id))
                .map(|record| record.to_coin_state())
                .collect()
        }

        fn coin_states_by_puzzle_hashes(
            &self,
            puzzle_hashes: &IndexSet<Bytes32>,
            include_hints: bool,
        ) -> Vec<CoinState> {
            self.state
                .coins
                .iter()
                .filter(|(coin_id, record)| {
                    puzzle_hashes.contains(&record.coin.puzzle_hash)
                        || (include_hints
                            && self
                                .state
                                .coin_hints
                                .get(*coin_id)
                                .is_some_and(|hint| puzzle_hashes.contains(hint)))
                })
                .map(|(_, record)| record.to_coin_state())
                .collect()
        }

        fn children(&self, coin_id: Bytes32) -> Vec<CoinState> {
            self.state
                .coins
                .values()
                .filter(|record| record.coin.parent_coin_info == coin_id)
                .map(|record| record.to_coin_state())
                .collect()
        }

        fn puzzle_and_solution(&self, coin_id: Bytes32, height: u32) -> Option<(Program, Program)> {
            let coin_spend = self
                .get_puzzle_and_solution(coin_id, Some(height))
                .coin_solution?;
            Some((coin_spend.puzzle_reveal, coin_spend.solution))
        }

        fn submit_transaction(
            &mut self,
            spend_bundle: SpendBundle,
        ) -> Result<Vec<PeerEvent>, SimulatorError> {
            self.normalize_and_insert(spend_bundle)?;
            Ok(Backend::drain_events(self))
        }

        fn drain_events(&mut self) -> Vec<PeerEvent> {
            std::mem::take(&mut self.peer_events.events)
        }

        fn mempool_items(&self) -> Vec<(Bytes32, MempoolItemIds)> {
            self.mempool
                .iter()
                .map(|(tx_id, item)| {
                    let ids = self
                        .peer_events
                        .item_ids
                        .get(tx_id)
                        .cloned()
                        .unwrap_or_else(|| self.mempool_item_ids(item));
                    (*tx_id, ids)
                })
                .collect()
        }

        fn mempool_transaction(&self, transaction_id: Bytes32) -> Option<SpendBundle> {
            self.mempool
                .get(&transaction_id)
                .map(|item| item.spend_bundle.clone())
        }
    }
}
