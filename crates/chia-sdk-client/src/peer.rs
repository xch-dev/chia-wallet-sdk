use std::{net::SocketAddr, sync::Arc, time::Duration};

use chia_protocol::{
    Bytes, Bytes32, ChiaProtocolMessage, CoinStateFilters, Message, ProtocolMessageTypes,
    PuzzleSolutionResponse, RegisterForCoinUpdates, RegisterForPhUpdates, RejectCoinState,
    RejectPuzzleSolution, RejectPuzzleState, RequestChildren, RequestCoinState, RequestPeers,
    RequestPuzzleSolution, RequestPuzzleState, RequestRemoveCoinSubscriptions,
    RequestRemovePuzzleSubscriptions, RequestTransaction, RespondChildren, RespondCoinState,
    RespondPeers, RespondPuzzleSolution, RespondPuzzleState, RespondRemoveCoinSubscriptions,
    RespondRemovePuzzleSubscriptions, RespondToCoinUpdates, RespondToPhUpdates, RespondTransaction,
    SendTransaction, SpendBundle, TransactionAck,
};
use chia_traits::Streamable;
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio::{
    net::TcpStream,
    sync::{
        Mutex,
        mpsc::{self, error::TrySendError},
        oneshot,
    },
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

#[derive(Debug, Clone, Copy)]
pub struct PeerOptions {
    pub rate_limit_factor: f64,
    /// How long to wait for a response to a request before giving up.
    /// `None` waits until the connection is closed.
    pub request_timeout: Option<Duration>,
    /// How long to wait for the connection and handshake to complete in `connect_peer`.
    /// `None` waits indefinitely.
    pub handshake_timeout: Option<Duration>,
    /// The number of unsolicited messages that can be buffered before the receiver is read.
    /// If the buffer fills up, the peer is disconnected rather than blocking responses to requests.
    pub notification_capacity: usize,
}

impl Default for PeerOptions {
    fn default() -> Self {
        Self {
            rate_limit_factor: 0.6,
            request_timeout: Some(Duration::from_secs(60)),
            handshake_timeout: Some(Duration::from_secs(30)),
            notification_capacity: 1024,
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
    request_timeout: Option<Duration>,
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
        let (sender, receiver) = mpsc::channel(options.notification_capacity.max(1));

        let requests = Arc::new(RequestMap::new());
        let requests_clone = requests.clone();

        let inbound_handle = tokio::spawn(async move {
            let result = handle_inbound_messages(stream, sender, &requests_clone).await;

            // Nothing will respond to pending or future requests once the inbound loop exits.
            requests_clone.close();

            if let Err(error) = result {
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
            request_timeout: options.request_timeout,
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
        let request = self.request_message(T::msg_type(), body.to_bytes()?.into());

        match self.0.request_timeout {
            Some(duration) => tokio::time::timeout(duration, request)
                .await
                .map_err(|_| ClientError::RequestTimeout(duration))?,
            None => request.await,
        }
    }

    async fn request_message(
        &self,
        msg_type: ProtocolMessageTypes,
        data: Bytes,
    ) -> Result<Message, ClientError> {
        let (sender, receiver) = oneshot::channel();
        let id = self.0.requests.insert(sender).await?;
        let pending = PendingRequest::new(&self.0.requests, id);

        self.send_raw(Message {
            msg_type,
            id: Some(id),
            data,
        })
        .await?;

        let response = receiver.await.map_err(|_| ClientError::ConnectionClosed)?;
        pending.complete();

        Ok(response)
    }

    async fn send_raw(&self, message: Message) -> Result<(), ClientError> {
        if self
            .0
            .outbound_rate_limiter
            .lock()
            .await
            .exceeds_size_limits(&message)
        {
            return Err(ClientError::MessageTooLarge(
                message.msg_type,
                message.data.len(),
            ));
        }

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

/// Removes the request from the map if it's cancelled or times out before a response arrives.
struct PendingRequest<'a> {
    requests: &'a RequestMap,
    id: u16,
    completed: bool,
}

impl<'a> PendingRequest<'a> {
    fn new(requests: &'a RequestMap, id: u16) -> Self {
        Self {
            requests,
            id,
            completed: false,
        }
    }

    fn complete(mut self) {
        self.completed = true;
    }
}

impl Drop for PendingRequest<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.requests.remove(self.id);
        }
    }
}

async fn handle_inbound_messages(
    mut stream: Stream,
    sender: mpsc::Sender<Message>,
    requests: &RequestMap,
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
                    // Waiting for space would stop responses to requests from being delivered,
                    // so disconnect instead if the receiver isn't keeping up.
                    if let Err(TrySendError::Full(message)) = sender.try_send(message) {
                        warn!(
                            "Notification channel is full, dropping {:?} message and disconnecting",
                            message.msg_type
                        );
                        return Err(ClientError::NotificationOverflow);
                    }
                    continue;
                };

                // This can happen if the request timed out or was cancelled before the response arrived.
                let Some(request) = requests.remove(id) else {
                    warn!(
                        "Received {:?} message with untracked id {id}",
                        message.msg_type
                    );
                    continue;
                };

                request.send(message);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chia_protocol::{Handshake, NodeType};
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    use super::*;

    type ServerSocket = WebSocketStream<TcpStream>;

    async fn connect(
        options: PeerOptions,
    ) -> anyhow::Result<(Peer, mpsc::Receiver<Message>, ServerSocket)> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await?;
            anyhow::Ok(accept_async(stream).await?)
        });

        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}")).await?;
        let (peer, receiver) = Peer::from_websocket(ws, options)?;

        Ok((peer, receiver, server.await??))
    }

    async fn recv_message(server: &mut ServerSocket) -> anyhow::Result<Message> {
        loop {
            let message = server.next().await.expect("connection closed")?;
            if let tungstenite::Message::Binary(bytes) = message {
                return Ok(Message::from_bytes(&bytes)?);
            }
        }
    }

    async fn send_message(server: &mut ServerSocket, message: Message) -> anyhow::Result<()> {
        server.send(message.to_bytes()?.into()).await?;
        Ok(())
    }

    fn respond_peers(id: Option<u16>) -> anyhow::Result<Message> {
        Ok(Message {
            msg_type: ProtocolMessageTypes::RespondPeers,
            id,
            data: RespondPeers::new(Vec::new()).to_bytes()?.into(),
        })
    }

    /// Fails the test instead of hanging if the future never completes.
    async fn within<F: Future>(future: F) -> F::Output {
        tokio::time::timeout(Duration::from_secs(10), future)
            .await
            .expect("future hung")
    }

    #[tokio::test]
    async fn test_pending_request_fails_when_connection_closes() -> anyhow::Result<()> {
        let options = PeerOptions {
            request_timeout: None,
            ..Default::default()
        };
        let (peer, _receiver, mut server) = connect(options).await?;

        let request = tokio::spawn({
            let peer = peer.clone();
            async move { peer.request_peers().await }
        });

        // Close the connection without responding to the request.
        recv_message(&mut server).await?;
        server.close(None).await?;

        let result = within(request).await?;
        assert!(matches!(result, Err(ClientError::ConnectionClosed)));

        // Requests made after the connection has closed fail immediately too.
        let result = within(peer.request_peers()).await;
        assert!(matches!(result, Err(ClientError::ConnectionClosed)));

        Ok(())
    }

    #[tokio::test]
    async fn test_request_timeout_and_late_response() -> anyhow::Result<()> {
        let timeout = Duration::from_millis(500);
        let options = PeerOptions {
            request_timeout: Some(timeout),
            ..Default::default()
        };
        let (peer, _receiver, mut server) = connect(options).await?;

        let result = within(peer.request_peers()).await;
        assert!(
            matches!(result, Err(ClientError::RequestTimeout(duration)) if duration == timeout)
        );

        let request = tokio::spawn({
            let peer = peer.clone();
            async move { peer.request_peers().await }
        });

        let first = recv_message(&mut server).await?;
        let second = recv_message(&mut server).await?;
        assert_ne!(first.id, second.id);

        // The late response to the timed out request is ignored rather than killing the connection.
        send_message(&mut server, respond_peers(first.id)?).await?;
        send_message(&mut server, respond_peers(second.id)?).await?;

        let response = within(request).await??;
        assert!(response.peer_list.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_message_too_large() -> anyhow::Result<()> {
        let (peer, _receiver, _server) = connect(PeerOptions::default()).await?;

        // Handshakes are limited to 10 KiB.
        let result = within(peer.send(Handshake {
            network_id: "x".repeat(20 * 1024),
            protocol_version: String::new(),
            software_version: String::new(),
            server_port: 0,
            node_type: NodeType::Wallet,
            capabilities: Vec::new(),
        }))
        .await;

        assert!(matches!(
            result,
            Err(ClientError::MessageTooLarge(
                ProtocolMessageTypes::Handshake,
                _
            ))
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_notification_overflow_disconnects() -> anyhow::Result<()> {
        let options = PeerOptions {
            request_timeout: None,
            notification_capacity: 2,
            ..Default::default()
        };
        let (peer, mut receiver, mut server) = connect(options).await?;

        for _ in 0..3 {
            send_message(&mut server, respond_peers(None)?).await?;
        }

        // The buffered notifications are still delivered, then the channel is closed.
        assert!(within(receiver.recv()).await.is_some());
        assert!(within(receiver.recv()).await.is_some());
        assert!(within(receiver.recv()).await.is_none());

        let result = within(peer.request_peers()).await;
        assert!(matches!(result, Err(ClientError::ConnectionClosed)));

        Ok(())
    }
}
