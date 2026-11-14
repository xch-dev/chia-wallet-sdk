use std::net::SocketAddr;

use chia_protocol::{
    Bytes32, ChiaProtocolMessage, CoinStateUpdate, MempoolItemsAdded, MempoolItemsRemoved, Message,
    NewPeakWallet, RemovedMempoolItem,
};
use chia_traits::Streamable;
use futures_util::SinkExt;
use indexmap::IndexSet;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::full_node_simulator::{MempoolItemIds, PeerEvent};

use super::{PeerSimulatorError, peer_map::PeerMap, subscriptions::Subscriptions};

/// The CHIP-26 capability that opts a peer into mempool updates.
pub(crate) const MEMPOOL_UPDATES_CAPABILITY: u16 = 5;

/// Serializes a protocol message that isn't a response to a request.
pub(crate) fn message<T>(body: &T) -> Result<WsMessage, PeerSimulatorError>
where
    T: Streamable + ChiaProtocolMessage,
{
    Ok(Message {
        msg_type: T::msg_type(),
        id: None,
        data: body.to_bytes()?.into(),
    }
    .to_bytes()?
    .into())
}

/// Sends each connected peer the messages for `events`, in order.
///
/// Every peer gets a `NewPeakWallet` for each block, and a `CoinStateUpdate` with the coins it's
/// subscribed to. Peers that opted into mempool updates also get `MempoolItemsAdded` and
/// `MempoolItemsRemoved` for the transactions that match their subscriptions. A peer that has
/// disconnected is skipped.
pub(crate) async fn notify_peers(
    peer_map: &PeerMap,
    subscriptions: &Subscriptions,
    events: &[PeerEvent],
) -> Result<(), PeerSimulatorError> {
    if events.is_empty() {
        return Ok(());
    }

    for (addr, mut peer) in peer_map.peers().await {
        for message in peer_messages(addr, peer.mempool_updates, subscriptions, events)? {
            if let Err(error) = peer.ws.send(message).await {
                tracing::debug!("skipping disconnected peer {addr}: {error}");
                break;
            }
        }
    }

    Ok(())
}

/// Returns a `MempoolItemsAdded` message for the mempool items that match `coin_ids` or
/// `puzzle_hashes`, or [`None`] if none do.
pub(crate) fn mempool_items_added(
    items: &[(Bytes32, MempoolItemIds)],
    coin_ids: &IndexSet<Bytes32>,
    puzzle_hashes: &IndexSet<Bytes32>,
) -> Result<Option<WsMessage>, PeerSimulatorError> {
    let transaction_ids: Vec<Bytes32> = items
        .iter()
        .filter(|(_, ids)| item_matches(ids, coin_ids, puzzle_hashes))
        .map(|(transaction_id, _)| *transaction_id)
        .collect();

    if transaction_ids.is_empty() {
        return Ok(None);
    }

    Ok(Some(message(&MempoolItemsAdded::new(transaction_ids))?))
}

fn item_matches(
    ids: &MempoolItemIds,
    coin_ids: &IndexSet<Bytes32>,
    puzzle_hashes: &IndexSet<Bytes32>,
) -> bool {
    ids.coin_ids.iter().any(|id| coin_ids.contains(id))
        || ids
            .puzzle_hashes
            .iter()
            .any(|ph| puzzle_hashes.contains(ph))
}

#[derive(Default)]
struct PendingMempoolUpdates {
    added: Vec<Bytes32>,
    removed: Vec<RemovedMempoolItem>,
}

impl PendingMempoolUpdates {
    fn flush(&mut self, messages: &mut Vec<WsMessage>) -> Result<(), PeerSimulatorError> {
        if !self.added.is_empty() {
            messages.push(message(&MempoolItemsAdded::new(std::mem::take(
                &mut self.added,
            )))?);
        }
        if !self.removed.is_empty() {
            messages.push(message(&MempoolItemsRemoved::new(std::mem::take(
                &mut self.removed,
            )))?);
        }
        Ok(())
    }
}

fn peer_messages(
    addr: SocketAddr,
    mempool_updates: bool,
    subscriptions: &Subscriptions,
    events: &[PeerEvent],
) -> Result<Vec<WsMessage>, PeerSimulatorError> {
    let empty = IndexSet::new();
    let coin_ids = subscriptions.coin_subscriptions(addr).unwrap_or(&empty);
    let puzzle_hashes = subscriptions.puzzle_subscriptions(addr).unwrap_or(&empty);

    let mut messages = Vec::new();
    let mut pending = PendingMempoolUpdates::default();

    for event in events {
        match event {
            PeerEvent::Block {
                height,
                header_hash,
                coin_states,
            } => {
                pending.flush(&mut messages)?;

                messages.push(message(&NewPeakWallet::new(
                    *header_hash,
                    *height,
                    0,
                    *height,
                ))?);

                let items: Vec<_> = coin_states
                    .iter()
                    .filter(|(coin_state, hints)| {
                        coin_ids.contains(&coin_state.coin.coin_id())
                            || puzzle_hashes.contains(&coin_state.coin.puzzle_hash)
                            || hints.iter().any(|hint| puzzle_hashes.contains(hint))
                    })
                    .map(|(coin_state, _)| *coin_state)
                    .collect();

                if !items.is_empty() {
                    messages.push(message(&CoinStateUpdate::new(
                        *height,
                        *height,
                        *header_hash,
                        items,
                    ))?);
                }
            }
            PeerEvent::MempoolItemAdded {
                transaction_id,
                ids,
            } => {
                if !mempool_updates || !item_matches(ids, coin_ids, puzzle_hashes) {
                    continue;
                }
                // Keep the order of additions and removals of the same transaction.
                if !pending.removed.is_empty() {
                    pending.flush(&mut messages)?;
                }
                pending.added.push(*transaction_id);
            }
            PeerEvent::MempoolItemRemoved {
                transaction_id,
                reason,
                ids,
            } => {
                if !mempool_updates || !item_matches(ids, coin_ids, puzzle_hashes) {
                    continue;
                }
                if !pending.added.is_empty() {
                    pending.flush(&mut messages)?;
                }
                pending
                    .removed
                    .push(RemovedMempoolItem::new(*transaction_id, *reason));
            }
        }
    }

    pending.flush(&mut messages)?;

    Ok(messages)
}
