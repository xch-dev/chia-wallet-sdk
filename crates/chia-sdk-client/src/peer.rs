use std::{net::SocketAddr, sync::Arc, time::Duration};

use chia_protocol::{
    Bytes32, ChiaProtocolMessage, CoinStateFilters, Message, PuzzleSolutionResponse,
    RegisterForCoinUpdates, RegisterForPhUpdates, RejectCoinState, RejectPuzzleSolution,
    RejectPuzzleState, RequestChildren, RequestCoinState, RequestPeers, RequestPuzzleSolution,
    RequestPuzzleState, RequestRemoveCoinSubscriptions, RequestRemovePuzzleSubscriptions,
    RequestTransaction, RespondChildren, RespondCoinState, RespondPeers, RespondPuzzleSolution,
    RespondPuzzleState, RespondRemoveCoinSubscriptions, RespondRemovePuzzleSubscriptions,
    RespondToCoinUpdates, RespondToPhUpdates, RespondTransaction, SendTransaction, SpendBundle,
    TransactionAck,
};
use chia_traits::Streamable;
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio::{
    net::TcpStream,
    sync::{Mutex, mpsc, oneshot},
    task::JoinHandle,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, warn};

use crate::{ClientError, RateLimiter, V2_RATE_LIMITS, request_map::RequestMap};

#[cfg(any(feature = "native-tls", feature = "rustls"))]
use tokio_tungstenite::Connector;

type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Sink = SplitSink<WebSocket, tungstenite::Message>;
type Stream = SplitStream<WebSocket>;
type Response<T, E> = std::result::Result<T, E>;

/// Options for a [`Peer`] connection.
///
/// # Example
///
/// ```
/// use chia_sdk_client::PeerOptions;
///
/// assert_eq!(PeerOptions::default().rate_limit_factor, 0.6);
/// assert!(!PeerOptions::default().mempool_updates);
///
/// // Use up to 90% of the full node's rate limits, and receive mempool updates.
/// let options = PeerOptions {
///     rate_limit_factor: 0.9,
///     mempool_updates: true,
/// };
/// # let _ = options;
/// ```
#[derive(Debug, Clone, Copy)]
pub struct PeerOptions {
    /// The fraction of the full node's rate limits ([`V2_RATE_LIMITS`]) that outgoing messages
    /// are allowed to use. Defaults to `0.6`, which leaves room so the full node doesn't
    /// disconnect the wallet for sending too many messages.
    pub rate_limit_factor: f64,

    /// Whether to advertise [`Capability::MempoolUpdates`](crate::Capability::MempoolUpdates)
    /// in the handshake, as defined in
    /// [CHIP-26](https://github.com/Chia-Network/chips/blob/main/CHIPs/chip-0026.md).
    /// Defaults to `false`.
    ///
    /// If the full node also supports it, it sends `MempoolItemsAdded` and
    /// `MempoolItemsRemoved` messages to the receiver for mempool transactions that match the
    /// wallet's puzzle hash and coin subscriptions. That includes the transactions already in
    /// the mempool when a subscription is added.
    ///
    /// This only affects the handshake sent by `connect_peer` and `Client::connect`.
    /// `Peer::connect` and [`Peer::from_websocket`] don't send a handshake, so they ignore it.
    pub mempool_updates: bool,
}

impl Default for PeerOptions {
    fn default() -> Self {
        Self {
            rate_limit_factor: 0.6,
            mempool_updates: false,
        }
    }
}

/// A connection to a Chia full node using the wallet protocol over a websocket.
///
/// A background task reads incoming messages. Responses to requests made with methods
/// like [`Peer::request_coin_state`] are returned from those methods, and every other message
/// (such as `CoinStateUpdate` or `NewPeakWallet`) is sent to the [`mpsc::Receiver`] that's
/// returned when the peer is created.
///
/// Keep reading from that receiver, or drop it. If 32 messages are waiting in it, the
/// background task stops processing messages, including responses, until there's room.
///
/// Outgoing messages are rate limited according to [`PeerOptions::rate_limit_factor`]. If a
/// message would exceed the limits, sending it waits until the next rate limit period.
///
/// Requests don't have a timeout, so wrap them in [`tokio::time::timeout`] if needed.
///
/// [`Peer`] is cheap to clone, and clones share the same connection. The background task is
/// stopped when the last clone is dropped.
///
/// # Example
///
/// This example is `no_run` because it connects to a live testnet11 peer.
///
/// ```no_run
/// # #[cfg(feature = "rustls")]
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use chia_protocol::{Bytes32, CoinStateFilters};
/// use chia_sdk_client::{Peer, PeerOptions, connect_peer, create_rustls_connector, load_ssl_cert};
/// use chia_sdk_types::TESTNET11_CONSTANTS;
///
/// let cert = load_ssl_cert("wallet.crt", "wallet.key")?;
/// let connector = create_rustls_connector(&cert)?;
///
/// // Performs the wallet protocol handshake after connecting.
/// let (peer, mut receiver) = connect_peer(
///     "testnet11".to_string(),
///     connector,
///     "127.0.0.1:58444".parse()?,
///     PeerOptions::default(),
/// )
/// .await?;
///
/// let puzzle_hash = Bytes32::new([0; 32]);
/// let response = peer
///     .request_puzzle_state(
///         vec![puzzle_hash],
///         None,
///         TESTNET11_CONSTANTS.genesis_challenge,
///         CoinStateFilters::new(true, true, true, 0),
///         true,
///     )
///     .await?;
///
/// match response {
///     Ok(state) => println!("Found {} coins", state.coin_states.len()),
///     Err(rejection) => println!("Rejected: {:?}", rejection.reason),
/// }
///
/// // Since `subscribe_when_finished` was true, updates are sent to the receiver.
/// while let Some(message) = receiver.recv().await {
///     println!("Received {:?}", message.msg_type);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Peer(Arc<PeerInner>);

#[derive(Debug)]
struct PeerInner {
    sink: Mutex<Sink>,
    inbound_handle: JoinHandle<()>,
    requests: Arc<RequestMap>,
    socket_addr: SocketAddr,
    outbound_rate_limiter: Mutex<RateLimiter>,
}

impl Peer {
    /// Opens a websocket connection to a peer at `wss://{socket_addr}/ws`.
    ///
    /// This doesn't perform the wallet protocol handshake, so most callers should use
    /// [`connect_peer`](crate::connect_peer) instead, or send a
    /// [`Handshake`](chia_protocol::Handshake) before making requests.
    #[cfg(any(feature = "native-tls", feature = "rustls"))]
    pub async fn connect(
        socket_addr: SocketAddr,
        connector: Connector,
        options: PeerOptions,
    ) -> Result<(Self, mpsc::Receiver<Message>), ClientError> {
        Self::connect_full_uri(&format!("wss://{socket_addr}/ws"), connector, options).await
    }

    /// Opens a websocket connection to a peer using its full websocket URI, for example
    /// `wss://127.0.0.1:8444/ws`.
    ///
    /// Like [`Peer::connect`], this doesn't perform the wallet protocol handshake.
    #[cfg(any(feature = "native-tls", feature = "rustls"))]
    pub async fn connect_full_uri(
        uri: &str,
        connector: Connector,
        options: PeerOptions,
    ) -> Result<(Self, mpsc::Receiver<Message>), ClientError> {
        let (ws, _) =
            tokio_tungstenite::connect_async_tls_with_config(uri, None, false, Some(connector))
                .await?;
        Self::from_websocket(ws, options)
    }

    /// Creates a peer from an existing websocket connection, and returns it with the receiver
    /// for messages that aren't responses to requests.
    ///
    /// The underlying stream can be plain TCP, or TLS from an enabled TLS feature. Otherwise,
    /// [`ClientError::UnsupportedTls`] is returned. Full nodes only accept TLS connections.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a Tokio runtime, since it spawns the background task.
    pub fn from_websocket(
        ws: WebSocket,
        options: PeerOptions,
    ) -> Result<(Self, mpsc::Receiver<Message>), ClientError> {
        let socket_addr = match ws.get_ref() {
            #[cfg(feature = "native-tls")]
            MaybeTlsStream::NativeTls(tls) => {
                let tls_stream = tls.get_ref();
                let tcp_stream = tls_stream.get_ref().get_ref();
                tcp_stream.peer_addr()?
            }
            #[cfg(feature = "rustls")]
            MaybeTlsStream::Rustls(tls) => {
                let (tcp_stream, _) = tls.get_ref();
                tcp_stream.peer_addr()?
            }
            MaybeTlsStream::Plain(plain) => plain.peer_addr()?,
            _ => return Err(ClientError::UnsupportedTls),
        };

        let (sink, stream) = ws.split();
        let (sender, receiver) = mpsc::channel(32);

        let requests = Arc::new(RequestMap::new());
        let requests_clone = requests.clone();

        let inbound_handle = tokio::spawn(async move {
            if let Err(error) = handle_inbound_messages(stream, sender, requests_clone).await {
                debug!("Error handling message: {error}");
            }
        });

        let peer = Self(Arc::new(PeerInner {
            sink: Mutex::new(sink),
            inbound_handle,
            requests,
            socket_addr,
            outbound_rate_limiter: Mutex::new(RateLimiter::new(
                false,
                60,
                options.rate_limit_factor,
                V2_RATE_LIMITS.clone(),
            )),
        }));

        Ok((peer, receiver))
    }

    /// The IP address and port of the peer.
    pub fn socket_addr(&self) -> SocketAddr {
        self.0.socket_addr
    }

    /// Submits a spend bundle to the peer's mempool with `SendTransaction`.
    ///
    /// The [`TransactionAck`] says whether the transaction was accepted, and the error if
    /// it wasn't. Acceptance doesn't mean that the transaction has been confirmed.
    pub async fn send_transaction(
        &self,
        spend_bundle: SpendBundle,
    ) -> Result<TransactionAck, ClientError> {
        self.request_infallible(SendTransaction::new(spend_bundle))
            .await
    }

    /// Requests the state of coins with the given puzzle hashes with `RequestPuzzleState`.
    ///
    /// Only coins that changed after `previous_height` are returned, or all coins if it's
    /// [`None`]. The `header_hash` must be the header hash of the block at `previous_height`,
    /// or the genesis challenge if it's [`None`], so the peer can detect reorgs. The `filters`
    /// choose which coins are included.
    ///
    /// Large results are split into pages. If the response's `is_finished` is `false`, request
    /// the next page with the response's `height` and `header_hash`. If
    /// `subscribe_when_finished` is `true`, the peer subscribes to updates for the puzzle
    /// hashes once the last page is sent, and sends them to the receiver.
    pub async fn request_puzzle_state(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        previous_height: Option<u32>,
        header_hash: Bytes32,
        filters: CoinStateFilters,
        subscribe_when_finished: bool,
    ) -> Result<Response<RespondPuzzleState, RejectPuzzleState>, ClientError> {
        self.request_fallible(RequestPuzzleState::new(
            puzzle_hashes,
            previous_height,
            header_hash,
            filters,
            subscribe_when_finished,
        ))
        .await
    }

    /// Requests the state of the given coins with `RequestCoinState`.
    ///
    /// Only coins that changed after `previous_height` are returned, or all of them if it's
    /// [`None`]. The `header_hash` must be the header hash of the block at `previous_height`,
    /// or the genesis challenge if it's [`None`]. If `subscribe` is `true`, the peer also
    /// sends updates for the coins to the receiver.
    pub async fn request_coin_state(
        &self,
        coin_ids: Vec<Bytes32>,
        previous_height: Option<u32>,
        header_hash: Bytes32,
        subscribe: bool,
    ) -> Result<Response<RespondCoinState, RejectCoinState>, ClientError> {
        self.request_fallible(RequestCoinState::new(
            coin_ids,
            previous_height,
            header_hash,
            subscribe,
        ))
        .await
    }

    /// Subscribes to updates for coins with the given puzzle hashes with `RegisterForPhUpdates`,
    /// and returns the state of those coins that changed since `min_height`.
    ///
    /// Later updates are sent to the receiver as `CoinStateUpdate` messages. Prefer
    /// [`Peer::request_puzzle_state`], which supports paging.
    pub async fn register_for_ph_updates(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        min_height: u32,
    ) -> Result<RespondToPhUpdates, ClientError> {
        self.request_infallible(RegisterForPhUpdates::new(puzzle_hashes, min_height))
            .await
    }

    /// Subscribes to updates for the given coins with `RegisterForCoinUpdates`, and returns
    /// the state of those coins that changed since `min_height`.
    ///
    /// Later updates are sent to the receiver as `CoinStateUpdate` messages.
    pub async fn register_for_coin_updates(
        &self,
        coin_ids: Vec<Bytes32>,
        min_height: u32,
    ) -> Result<RespondToCoinUpdates, ClientError> {
        self.request_infallible(RegisterForCoinUpdates::new(coin_ids, min_height))
            .await
    }

    /// Unsubscribes from updates for the given puzzle hashes, or from all puzzle hashes if
    /// `puzzle_hashes` is [`None`].
    pub async fn remove_puzzle_subscriptions(
        &self,
        puzzle_hashes: Option<Vec<Bytes32>>,
    ) -> Result<RespondRemovePuzzleSubscriptions, ClientError> {
        self.request_infallible(RequestRemovePuzzleSubscriptions::new(puzzle_hashes))
            .await
    }

    /// Unsubscribes from updates for the given coins, or from all coins if `coin_ids` is
    /// [`None`].
    pub async fn remove_coin_subscriptions(
        &self,
        coin_ids: Option<Vec<Bytes32>>,
    ) -> Result<RespondRemoveCoinSubscriptions, ClientError> {
        self.request_infallible(RequestRemoveCoinSubscriptions::new(coin_ids))
            .await
    }

    /// Requests the spend bundle with the given transaction id from the peer's mempool with
    /// `RequestTransaction`.
    pub async fn request_transaction(
        &self,
        transaction_id: Bytes32,
    ) -> Result<RespondTransaction, ClientError> {
        self.request_infallible(RequestTransaction::new(transaction_id))
            .await
    }

    /// Requests the puzzle reveal and solution of a spent coin with `RequestPuzzleSolution`.
    ///
    /// The `height` must be the height of the block that spent the coin, otherwise the request
    /// is rejected.
    pub async fn request_puzzle_and_solution(
        &self,
        coin_id: Bytes32,
        height: u32,
    ) -> Result<Response<PuzzleSolutionResponse, RejectPuzzleSolution>, ClientError> {
        match self
            .request_fallible::<RespondPuzzleSolution, _, _>(RequestPuzzleSolution::new(
                coin_id, height,
            ))
            .await?
        {
            Ok(response) => Ok(Ok(response.response)),
            Err(rejection) => Ok(Err(rejection)),
        }
    }

    /// Requests the state of the coins created by spending the given coin with
    /// `RequestChildren`.
    pub async fn request_children(&self, coin_id: Bytes32) -> Result<RespondChildren, ClientError> {
        self.request_infallible(RequestChildren::new(coin_id)).await
    }

    /// Requests the addresses of other full nodes that the peer knows about with
    /// `RequestPeers`.
    pub async fn request_peers(&self) -> Result<RespondPeers, ClientError> {
        self.request_infallible(RequestPeers::new()).await
    }

    /// Sends a message to the peer without a request id, and doesn't wait for a response.
    pub async fn send<T>(&self, body: T) -> Result<(), ClientError>
    where
        T: Streamable + ChiaProtocolMessage,
    {
        self.send_raw(Message {
            msg_type: T::msg_type(),
            id: None,
            data: body.to_bytes()?.into(),
        })
        .await?;

        Ok(())
    }

    /// Sends a request to the peer and waits for a response of type `T` or a rejection of
    /// type `E`.
    ///
    /// Returns [`ClientError::InvalidResponse`] if the peer responds with any other message type.
    pub async fn request_fallible<T, E, B>(&self, body: B) -> Result<Response<T, E>, ClientError>
    where
        T: Streamable + ChiaProtocolMessage,
        E: Streamable + ChiaProtocolMessage,
        B: Streamable + ChiaProtocolMessage,
    {
        let message = self.request_raw(body).await?;
        if message.msg_type != T::msg_type() && message.msg_type != E::msg_type() {
            return Err(ClientError::InvalidResponse(
                vec![T::msg_type(), E::msg_type()],
                message.msg_type,
            ));
        }
        if message.msg_type == T::msg_type() {
            Ok(Ok(T::from_bytes(&message.data)?))
        } else {
            Ok(Err(E::from_bytes(&message.data)?))
        }
    }

    /// Sends a request to the peer and waits for a response of type `T`.
    ///
    /// Returns [`ClientError::InvalidResponse`] if the peer responds with any other message type.
    pub async fn request_infallible<T, B>(&self, body: B) -> Result<T, ClientError>
    where
        T: Streamable + ChiaProtocolMessage,
        B: Streamable + ChiaProtocolMessage,
    {
        let message = self.request_raw(body).await?;
        if message.msg_type != T::msg_type() {
            return Err(ClientError::InvalidResponse(
                vec![T::msg_type()],
                message.msg_type,
            ));
        }
        Ok(T::from_bytes(&message.data)?)
    }

    /// Sends a request to the peer and returns the response message without parsing it.
    ///
    /// The request is assigned an unused request id, and the response is matched to it by id.
    pub async fn request_raw<T>(&self, body: T) -> Result<Message, ClientError>
    where
        T: Streamable + ChiaProtocolMessage,
    {
        let (sender, receiver) = oneshot::channel();

        self.send_raw(Message {
            msg_type: T::msg_type(),
            id: Some(self.0.requests.insert(sender).await),
            data: body.to_bytes()?.into(),
        })
        .await?;

        Ok(receiver.await?)
    }

    async fn send_raw(&self, message: Message) -> Result<(), ClientError> {
        loop {
            if !self
                .0
                .outbound_rate_limiter
                .lock()
                .await
                .handle_message(&message)
            {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            self.0
                .sink
                .lock()
                .await
                .send(message.to_bytes()?.into())
                .await?;

            return Ok(());
        }
    }

    /// Closes the websocket connection.
    ///
    /// This affects every clone of the peer, and messages can't be sent afterwards.
    pub async fn close(&self) -> Result<(), ClientError> {
        self.0.sink.lock().await.close().await?;
        Ok(())
    }
}

impl Drop for PeerInner {
    fn drop(&mut self) {
        self.inbound_handle.abort();
    }
}

async fn handle_inbound_messages(
    mut stream: Stream,
    sender: mpsc::Sender<Message>,
    requests: Arc<RequestMap>,
) -> Result<(), ClientError> {
    use tungstenite::Message::{Binary, Close, Frame, Ping, Pong, Text};

    while let Some(message) = stream.next().await {
        let message = message?;

        match message {
            Frame(..) => unreachable!(),
            Close(..) => break,
            Ping(..) | Pong(..) => {}
            Text(text) => {
                warn!("Received unexpected text message: {text}");
            }
            Binary(binary) => {
                let message = Message::from_bytes(&binary)?;

                let Some(id) = message.id else {
                    sender.send(message).await.ok();
                    continue;
                };

                let Some(request) = requests.remove(id).await else {
                    warn!(
                        "Received {:?} message with untracked id {id}",
                        message.msg_type
                    );
                    return Err(ClientError::UnexpectedMessage(message.msg_type));
                };

                request.send(message);
            }
        }
    }
    Ok(())
}
