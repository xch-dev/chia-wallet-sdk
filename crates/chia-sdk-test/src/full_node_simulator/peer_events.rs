//! Changes that a [`PeerSimulator`](crate::PeerSimulator) serving this simulator announces to
//! connected peers. Nothing is recorded unless the peer simulator enables the log.

use chia_consensus::validation_error::ErrorCode;
use chia_protocol::{Bytes32, CoinState, MempoolRemoveReason};
use indexmap::IndexSet;

use crate::SimulatorError;

use super::{FullNodeSimulator, SimCoinRecord, ValidatedBundle};

#[derive(Debug, Clone, Default)]
#[cfg_attr(not(feature = "peer-simulator"), allow(dead_code))]
pub(crate) struct PeerEventLog {
    enabled: bool,
    events: Vec<PeerEvent>,
}

/// A change to the chain or the mempool, in the order it happened.
#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "peer-simulator"), allow(dead_code))]
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
#[cfg_attr(not(feature = "peer-simulator"), allow(dead_code))]
pub struct MempoolItemIds {
    /// The ids of the coins the item spends and creates.
    pub coin_ids: IndexSet<Bytes32>,
    /// The puzzle hashes of the coins the item spends and creates, and the hints of the coins it
    /// creates.
    pub puzzle_hashes: IndexSet<Bytes32>,
}

impl MempoolItemIds {
    fn new(item: &ValidatedBundle) -> Self {
        let mut ids = Self::default();
        for spend in item.spends.values() {
            ids.coin_ids.insert(spend.coin_spend.coin.coin_id());
            ids.puzzle_hashes.insert(spend.coin_spend.coin.puzzle_hash);
        }
        for (coin, hint) in &item.additions {
            ids.coin_ids.insert(coin.coin_id());
            ids.puzzle_hashes.insert(coin.puzzle_hash);
            ids.puzzle_hashes.extend(*hint);
        }
        ids
    }
}

impl SimCoinRecord {
    fn to_coin_state(self) -> CoinState {
        CoinState::new(
            self.coin,
            self.spent_block_index,
            Some(self.confirmed_block_index),
        )
    }
}

impl FullNodeSimulator {
    pub(super) fn record_block_for_peers(
        &mut self,
        height: u32,
        header_hash: Bytes32,
        coin_ids: &[Bytes32],
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
        let ids = MempoolItemIds::new(item);
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
        self.peer_events.events.push(PeerEvent::MempoolItemRemoved {
            transaction_id,
            reason,
            ids: MempoolItemIds::new(item),
        });
    }

    /// Stops recording peer events, and returns whether they were being recorded.
    pub(super) fn pause_peer_events(&mut self) -> bool {
        std::mem::replace(&mut self.peer_events.enabled, false)
    }

    pub(super) fn resume_peer_events(&mut self, enabled: bool) {
        self.peer_events.enabled = enabled;
    }

    /// The reason a mempool item that fails validation with `error` is removed: [`Expired`] if
    /// one of its `ASSERT_BEFORE_*` conditions no longer passes, otherwise [`Conflict`].
    ///
    /// Items are currently never left out of a block while they're valid, so they can't expire
    /// in practice. The mapping is here for when that changes, such as with a block cost limit.
    ///
    /// [`Expired`]: MempoolRemoveReason::Expired
    /// [`Conflict`]: MempoolRemoveReason::Conflict
    pub(super) fn removal_reason(error: &SimulatorError) -> MempoolRemoveReason {
        match error {
            SimulatorError::Validation(
                ErrorCode::AssertBeforeHeightAbsoluteFailed
                | ErrorCode::AssertBeforeHeightRelativeFailed
                | ErrorCode::AssertBeforeSecondsAbsoluteFailed
                | ErrorCode::AssertBeforeSecondsRelativeFailed,
            ) => MempoolRemoveReason::Expired,
            _ => MempoolRemoveReason::Conflict,
        }
    }

    /// Removes the mempool items that are no longer valid at the current peak, such as items that
    /// spend a coin a block just spent, or whose `ASSERT_BEFORE_*` conditions have expired.
    ///
    /// Items that depend on a removed item are removed as well.
    pub(super) fn evict_invalid_mempool_items(&mut self) {
        loop {
            let invalid = self
                .mempool
                .iter()
                .filter_map(|(tx_id, item)| {
                    self.validate_bundle(item.spend_bundle.clone())
                        .err()
                        .map(|error| (*tx_id, Self::removal_reason(&error)))
                })
                .collect::<Vec<_>>();
            if invalid.is_empty() {
                break;
            }
            for (tx_id, reason) in invalid {
                if let Some(item) = self.mempool.shift_remove(&tx_id) {
                    self.record_mempool_removed(tx_id, &item, reason);
                }
            }
        }
    }
}

#[cfg(feature = "peer-simulator")]
impl FullNodeSimulator {
    pub(crate) fn enable_peer_events(&mut self) {
        self.peer_events.enabled = true;
    }

    pub(crate) fn drain_peer_events(&mut self) -> Vec<PeerEvent> {
        std::mem::take(&mut self.peer_events.events)
    }

    pub(crate) fn peer_coin_states(&self, coin_ids: &IndexSet<Bytes32>) -> Vec<CoinState> {
        coin_ids
            .iter()
            .filter_map(|coin_id| self.state.coins.get(coin_id))
            .map(|record| record.to_coin_state())
            .collect()
    }

    pub(crate) fn peer_coin_states_by_puzzle_hashes(
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

    pub(crate) fn peer_children(&self, coin_id: Bytes32) -> Vec<CoinState> {
        self.state
            .coins
            .values()
            .filter(|record| record.coin.parent_coin_info == coin_id)
            .map(|record| record.to_coin_state())
            .collect()
    }

    pub(crate) fn peer_coin_spend(&self, coin_id: Bytes32) -> Option<&chia_protocol::CoinSpend> {
        self.state.coin_spends.get(&coin_id)
    }

    pub(crate) fn peer_mempool_items(&self) -> Vec<(Bytes32, MempoolItemIds)> {
        self.mempool
            .iter()
            .map(|(tx_id, item)| (*tx_id, MempoolItemIds::new(item)))
            .collect()
    }

    pub(crate) fn peer_mempool_spend_bundle(
        &self,
        transaction_id: Bytes32,
    ) -> Option<chia_protocol::SpendBundle> {
        self.mempool
            .get(&transaction_id)
            .map(|item| item.spend_bundle.clone())
    }
}
