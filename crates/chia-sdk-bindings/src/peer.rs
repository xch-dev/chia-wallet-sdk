use std::sync::Arc;

use bindy::Result;
use chia_protocol::{
    Bytes32, CoinStateUpdate, MempoolItemsAdded, MempoolItemsRemoved, Message, NewPeakWallet,
    ProtocolMessageTypes, PuzzleSolutionResponse, RespondChildren, RespondCoinState, RespondPeers,
    RespondPuzzleState, RespondToCoinUpdates, RespondToPhUpdates, RespondTransaction, SpendBundle,
    TransactionAck,
};
use chia_sdk_client::{
    Connector as SdkConnector, Peer as SdkPeer, PeerOptions as SdkPeerOptions, connect_peer,
    create_native_tls_connector, load_ssl_cert,
};
use chia_ssl::ChiaCertificate;
use chia_traits::Streamable;
use tokio::sync::{Mutex, mpsc::Receiver};

#[derive(Clone)]
pub struct Certificate {
    pub cert_pem: String,
    pub key_pem: String,
}

impl Certificate {
    pub fn load(cert_path: String, key_path: String) -> Result<Self> {
        let cert = load_ssl_cert(&cert_path, &key_path)?;
        Ok(Self {
            cert_pem: cert.cert_pem,
            key_pem: cert.key_pem,
        })
    }

    pub fn generate() -> Result<Self> {
        let cert = ChiaCertificate::generate()?;
        Ok(Self {
            cert_pem: cert.cert_pem,
            key_pem: cert.key_pem,
        })
    }
}

#[derive(Clone)]
pub struct Connector(pub(crate) SdkConnector);

impl Connector {
    pub fn new(cert: Certificate) -> Result<Self> {
        let connector = create_native_tls_connector(&ChiaCertificate {
            cert_pem: cert.cert_pem,
            key_pem: cert.key_pem,
        })?;
        Ok(Self(connector))
    }
}

#[derive(Clone)]
pub struct PeerOptions {
    pub rate_limit_factor: f64,
    pub mempool_updates: bool,
}

impl PeerOptions {
    pub fn new() -> Result<Self> {
        Ok(SdkPeerOptions::default().into())
    }
}

impl From<SdkPeerOptions> for PeerOptions {
    fn from(options: SdkPeerOptions) -> Self {
        Self {
            rate_limit_factor: options.rate_limit_factor,
            mempool_updates: options.mempool_updates,
        }
    }
}

impl From<PeerOptions> for SdkPeerOptions {
    fn from(options: PeerOptions) -> Self {
        Self {
            rate_limit_factor: options.rate_limit_factor,
            mempool_updates: options.mempool_updates,
        }
    }
}

#[derive(Clone)]
pub struct CoinStateFilters {
    pub include_spent: bool,
    pub include_unspent: bool,
    pub include_hinted: bool,
    pub min_amount: u64,
}

impl From<CoinStateFilters> for chia_protocol::CoinStateFilters {
    fn from(value: CoinStateFilters) -> Self {
        Self {
            include_spent: value.include_spent,
            include_unspent: value.include_unspent,
            include_hinted: value.include_hinted,
            min_amount: value.min_amount,
        }
    }
}

#[derive(Clone)]
pub struct Peer(SdkPeer, Arc<Mutex<Receiver<Message>>>);

impl Peer {
    pub async fn connect(
        network_id: String,
        socket_addr: String,
        connector: Connector,
        options: PeerOptions,
    ) -> Result<Self> {
        let (peer, receiver) = connect_peer(
            network_id,
            connector.0.clone(),
            socket_addr.parse()?,
            options.into(),
        )
        .await?;

        Ok(Self::from_parts(peer, receiver))
    }

    /// Wraps a connected peer and the receiver for its incoming messages, so that the receiver
    /// can be read with [`Peer::next`].
    pub(crate) fn from_parts(peer: SdkPeer, receiver: Receiver<Message>) -> Self {
        Self(peer, Arc::new(Mutex::new(receiver)))
    }

    pub async fn request_coin_state(
        &self,
        coin_ids: Vec<Bytes32>,
        previous_height: Option<u32>,
        header_hash: Bytes32,
        subscribe: bool,
    ) -> Result<RespondCoinState> {
        self.0
            .request_coin_state(coin_ids, previous_height, header_hash, subscribe)
            .await?
            .map_err(bindy::Error::RejectCoinState)
    }

    pub async fn request_puzzle_state(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        previous_height: Option<u32>,
        header_hash: Bytes32,
        filters: CoinStateFilters,
        subscribe: bool,
    ) -> Result<RespondPuzzleState> {
        self.0
            .request_puzzle_state(
                puzzle_hashes,
                previous_height,
                header_hash,
                filters.into(),
                subscribe,
            )
            .await?
            .map_err(bindy::Error::RejectPuzzleState)
    }

    pub async fn request_puzzle_and_solution(
        &self,
        coin_id: Bytes32,
        height: u32,
    ) -> Result<PuzzleSolutionResponse> {
        self.0
            .request_puzzle_and_solution(coin_id, height)
            .await?
            .map_err(bindy::Error::RejectPuzzleSolution)
    }

    pub async fn remove_coin_subscriptions(
        &self,
        coin_ids: Option<Vec<Bytes32>>,
    ) -> Result<Vec<Bytes32>> {
        Ok(self.0.remove_coin_subscriptions(coin_ids).await?.coin_ids)
    }

    pub async fn remove_puzzle_subscriptions(
        &self,
        puzzle_hashes: Option<Vec<Bytes32>>,
    ) -> Result<Vec<Bytes32>> {
        Ok(self
            .0
            .remove_puzzle_subscriptions(puzzle_hashes)
            .await?
            .puzzle_hashes)
    }

    /// Submits a spend bundle to the peer's mempool.
    ///
    /// A rejected transaction isn't an error: check the returned ack's `status` and `error`.
    pub async fn send_transaction(&self, spend_bundle: SpendBundle) -> Result<TransactionAck> {
        Ok(self.0.send_transaction(spend_bundle).await?)
    }

    /// Subscribes to updates for coins with the given puzzle hashes, and returns the state of
    /// those coins that changed since `min_height`. Later updates are returned by [`Peer::next`].
    pub async fn register_for_ph_updates(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        min_height: u32,
    ) -> Result<RespondToPhUpdates> {
        Ok(self
            .0
            .register_for_ph_updates(puzzle_hashes, min_height)
            .await?)
    }

    /// Subscribes to updates for the given coins, and returns the state of those coins that
    /// changed since `min_height`. Later updates are returned by [`Peer::next`].
    pub async fn register_for_coin_updates(
        &self,
        coin_ids: Vec<Bytes32>,
        min_height: u32,
    ) -> Result<RespondToCoinUpdates> {
        Ok(self
            .0
            .register_for_coin_updates(coin_ids, min_height)
            .await?)
    }

    /// Requests the state of the coins created by spending the given coin.
    pub async fn request_children(&self, coin_id: Bytes32) -> Result<RespondChildren> {
        Ok(self.0.request_children(coin_id).await?)
    }

    /// Requests the spend bundle with the given transaction id from the peer's mempool.
    ///
    /// Full nodes don't respond if the transaction isn't in their mempool, so this waits until
    /// the connection closes.
    pub async fn request_transaction(&self, transaction_id: Bytes32) -> Result<RespondTransaction> {
        Ok(self.0.request_transaction(transaction_id).await?)
    }

    /// Requests the addresses of other full nodes that the peer knows about.
    pub async fn request_peers(&self) -> Result<RespondPeers> {
        Ok(self.0.request_peers().await?)
    }

    /// The IP address and port of the peer, such as `127.0.0.1:8444`.
    pub fn socket_addr(&self) -> Result<String> {
        Ok(self.0.socket_addr().to_string())
    }

    /// Closes the connection. This affects every clone of the peer, and later requests fail.
    pub async fn close(&self) -> Result<()> {
        self.0.close().await?;
        Ok(())
    }

    pub async fn next(&self) -> Result<Option<Event>> {
        let mut receiver = self.1.lock().await;

        while let Some(message) = receiver.recv().await {
            if let Some(event) = Event::from_message(&message)? {
                return Ok(Some(event));
            }
        }

        Ok(None)
    }
}

/// A message from the peer that isn't a response to a request. Exactly one field is set.
///
/// `mempool_items_added` and `mempool_items_removed` are only sent when
/// [`PeerOptions::mempool_updates`] is enabled and the full node supports CHIP-26.
#[derive(Clone, Default)]
pub struct Event {
    pub new_peak_wallet: Option<NewPeakWallet>,
    pub coin_state_update: Option<CoinStateUpdate>,
    pub mempool_items_added: Option<MempoolItemsAdded>,
    pub mempool_items_removed: Option<MempoolItemsRemoved>,
}

impl Event {
    /// Decodes a message into an event, or returns `None` for message types that aren't
    /// events.
    fn from_message(message: &Message) -> Result<Option<Self>> {
        let data = &message.data;

        Ok(Some(match message.msg_type {
            ProtocolMessageTypes::NewPeakWallet => Self {
                new_peak_wallet: Some(NewPeakWallet::from_bytes(data)?),
                ..Self::default()
            },
            ProtocolMessageTypes::CoinStateUpdate => Self {
                coin_state_update: Some(CoinStateUpdate::from_bytes(data)?),
                ..Self::default()
            },
            ProtocolMessageTypes::MempoolItemsAdded => Self {
                mempool_items_added: Some(MempoolItemsAdded::from_bytes(data)?),
                ..Self::default()
            },
            ProtocolMessageTypes::MempoolItemsRemoved => Self {
                mempool_items_removed: Some(MempoolItemsRemoved::from_bytes(data)?),
                ..Self::default()
            },
            _ => return Ok(None),
        }))
    }
}

pub trait RespondCoinStateExt {}

pub trait RespondPuzzleStateExt {}

pub trait PuzzleSolutionResponseExt {}

pub trait CoinStateUpdateExt {}

pub trait NewPeakWalletExt {}

pub trait TransactionAckExt {}

pub trait RespondToPhUpdatesExt {}

pub trait RespondToCoinUpdatesExt {}

pub trait RespondChildrenExt {}

pub trait RespondTransactionExt {}

pub trait RespondPeersExt {}

pub trait TimestampedPeerInfoExt {}

pub trait MempoolItemsAddedExt {}

pub trait MempoolItemsRemovedExt {}

pub trait RemovedMempoolItemExt {}

#[cfg(test)]
mod tests {
    use chia_protocol::{Bytes, MempoolRemoveReason, NewPeakWallet, RemovedMempoolItem};

    use super::*;

    fn message<T: Streamable>(msg_type: ProtocolMessageTypes, value: &T) -> Message {
        Message {
            msg_type,
            id: None,
            data: Bytes::new(value.to_bytes().unwrap()),
        }
    }

    #[test]
    fn test_peer_options_roundtrip() {
        let options = PeerOptions::new().unwrap();
        assert!(!options.mempool_updates);

        let options = SdkPeerOptions::from(PeerOptions {
            rate_limit_factor: 0.9,
            mempool_updates: true,
        });
        assert!(options.mempool_updates);
        assert!((options.rate_limit_factor - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_mempool_events() {
        let added = MempoolItemsAdded::new(vec![Bytes32::new([1; 32])]);
        let event = Event::from_message(&message(ProtocolMessageTypes::MempoolItemsAdded, &added))
            .unwrap()
            .unwrap();
        assert_eq!(event.mempool_items_added, Some(added));
        assert!(event.mempool_items_removed.is_none());
        assert!(event.new_peak_wallet.is_none());
        assert!(event.coin_state_update.is_none());

        let removed = MempoolItemsRemoved::new(vec![RemovedMempoolItem::new(
            Bytes32::new([2; 32]),
            MempoolRemoveReason::BlockInclusion,
        )]);
        let event = Event::from_message(&message(
            ProtocolMessageTypes::MempoolItemsRemoved,
            &removed,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(event.mempool_items_removed, Some(removed));
        assert!(event.mempool_items_added.is_none());

        let peak = NewPeakWallet::new(Bytes32::new([3; 32]), 1, 2, 0);
        let event = Event::from_message(&message(ProtocolMessageTypes::NewPeakWallet, &peak))
            .unwrap()
            .unwrap();
        assert_eq!(event.new_peak_wallet, Some(peak));
        assert!(event.mempool_items_added.is_none());
    }

    #[test]
    fn test_other_messages_are_not_events() {
        let message = message(
            ProtocolMessageTypes::RespondCostInfo,
            &chia_protocol::RespondCostInfo::new(0, 0, 0, 0, 0, 0),
        );
        assert!(Event::from_message(&message).unwrap().is_none());
    }
}
