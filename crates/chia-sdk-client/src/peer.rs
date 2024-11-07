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
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use tokio::{
    net::TcpStream,
    sync::{mpsc, oneshot, Mutex},
    task::JoinHandle,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, warn};

use crate::{request_map::RequestMap, ClientError, RateLimiter, V2_RATE_LIMITS};

#[cfg(any(feature = "native-tls", feature = "rustls"))]
use tokio_tungstenite::Connector;

type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Sink = SplitSink<WebSocket, tungstenite::Message>;
type Stream = SplitStream<WebSocket>;
type Response<T, E> = std::result::Result<T, E>;

#[derive(Debug, Clone, Copy)]
pub struct PeerOptions {
    pub rate_limit_factor: f64,
}

impl Default for PeerOptions {
    fn default() -> Self {
        Self {
            rate_limit_factor: 0.6,
        }
    }
}

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
    /// Connects to a peer using its IP address and port.
    #[cfg(any(feature = "native-tls", feature = "rustls"))]
    pub async fn connect(
        socket_addr: SocketAddr,
        connector: Connector,
        options: PeerOptions,
    ) -> Result<(Self, mpsc::Receiver<Message>), ClientError> {
        Self::connect_full_uri(&format!("wss://{socket_addr}/ws"), connector, options).await
    }

    /// Connects to a peer using its full websocket URI.
    /// For example, `wss://127.0.0.1:8444/ws`.
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

    /// Creates a peer from an existing websocket connection.
    /// The connection must be secured with TLS, so that the certificate can be hashed in a peer id.
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

    /// The IP address and port of the peer connection.
    pub fn socket_addr(&self) -> SocketAddr {
        self.0.socket_addr
    }

    pub async fn send_transaction(
        &self,
        spend_bundle: SpendBundle,
    ) -> Result<TransactionAck, ClientError> {
        self.request_infallible(SendTransaction::new(spend_bundle))
            .await
    }

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

    pub async fn register_for_ph_updates(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        min_height: u32,
    ) -> Result<RespondToPhUpdates, ClientError> {
        self.request_infallible(RegisterForPhUpdates::new(puzzle_hashes, min_height))
            .await
    }

    pub async fn register_for_coin_updates(
        &self,
        coin_ids: Vec<Bytes32>,
        min_height: u32,
    ) -> Result<RespondToCoinUpdates, ClientError> {
        self.request_infallible(RegisterForCoinUpdates::new(coin_ids, min_height))
            .await
    }

    pub async fn remove_puzzle_subscriptions(
        &self,
        puzzle_hashes: Option<Vec<Bytes32>>,
    ) -> Result<RespondRemovePuzzleSubscriptions, ClientError> {
        self.request_infallible(RequestRemovePuzzleSubscriptions::new(puzzle_hashes))
            .await
    }

    pub async fn remove_coin_subscriptions(
        &self,
        coin_ids: Option<Vec<Bytes32>>,
    ) -> Result<RespondRemoveCoinSubscriptions, ClientError> {
        self.request_infallible(RequestRemoveCoinSubscriptions::new(coin_ids))
            .await
    }

    pub async fn request_transaction(
        &self,
        transaction_id: Bytes32,
    ) -> Result<RespondTransaction, ClientError> {
        self.request_infallible(RequestTransaction::new(transaction_id))
            .await
    }

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

    pub async fn request_children(&self, coin_id: Bytes32) -> Result<RespondChildren, ClientError> {
        self.request_infallible(RequestChildren::new(coin_id)).await
    }

    pub async fn request_peers(&self) -> Result<RespondPeers, ClientError> {
        self.request_infallible(RequestPeers::new()).await
    }

    /// Sends a message to the peer, but does not expect any response.
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

    /// Sends a message to the peer and expects a message that's either a response or a rejection.
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

    /// Sends a message to the peer and expects a specific response message.
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

    /// Sends a message to the peer and expects any arbitrary protocol message without parsing it.
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
