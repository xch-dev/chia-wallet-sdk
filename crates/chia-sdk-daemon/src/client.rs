#[cfg(any(feature = "native-tls", feature = "rustls"))]
mod inner {
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicU32, AtomicU64, Ordering},
        },
        time::Duration,
    };

    use chia_sdk_coinset::ChiaRpcClient;
    use futures_util::{
        SinkExt, StreamExt,
        stream::{SplitSink, SplitStream},
    };
    use serde::{Serialize, de::DeserializeOwned};
    use tokio::{
        net::TcpStream,
        sync::{Mutex, broadcast, oneshot, watch},
        task::JoinHandle,
    };
    use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};
    use tracing::{debug, error, info, warn};

    use crate::{DaemonError, DaemonEvent, WebsocketRequest, WebsocketResponse};

    type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;
    type Sink = SplitSink<WebSocket, tungstenite::Message>;
    type Stream = SplitStream<WebSocket>;

    static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn next_request_id() -> String {
        REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed).to_string()
    }

    /// A websocket client for the Chia daemon, which routes requests to the services it runs.
    ///
    /// [`ChiaRpcClient`] is implemented by sending each RPC request to the `chia_full_node`
    /// service, so the full node RPC methods can be used through the daemon.
    ///
    /// A background task reads messages from the daemon. Responses are matched to requests by
    /// id, and every other message is broadcast to the receivers returned by
    /// [`DaemonClient::subscribe`].
    ///
    /// If the connection fails, the client reconnects with exponential backoff, from 1 second
    /// up to 30 seconds between attempts, and registers its services again. Pending requests
    /// fail with [`DaemonError::ReceiveFailed`], and new requests fail with
    /// [`DaemonError::ConnectionClosed`] until it reconnects. If the daemon closes the
    /// connection cleanly, the client doesn't reconnect.
    ///
    /// [`DaemonClient`] is cheap to clone, and clones share the same connection.
    ///
    /// # Example
    ///
    /// This example is `no_run` because it connects to a Chia daemon running locally.
    ///
    /// ```no_run
    /// # #[cfg(feature = "rustls")]
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::time::Duration;
    ///
    /// use chia_sdk_client::{create_rustls_connector, load_ssl_cert};
    /// use chia_sdk_coinset::ChiaRpcClient;
    /// use chia_sdk_daemon::DaemonClient;
    ///
    /// // Use the daemon's existing certificate from `config/ssl/daemon` in the Chia root
    /// // directory. Note that `load_ssl_cert` generates new files if these can't be read.
    /// let cert = load_ssl_cert("private_daemon.crt", "private_daemon.key")?;
    /// let connector = create_rustls_connector(&cert)?;
    ///
    /// let client =
    ///     DaemonClient::connect("wss://localhost:55400", connector, Duration::from_secs(30)).await?;
    ///
    /// let response = client.get_blockchain_state().await?;
    /// if let Some(state) = response.blockchain_state {
    ///     println!("Peak height: {}", state.peak.height);
    /// }
    ///
    /// let mut events = client.subscribe("wallet_ui").await?;
    /// while let Ok(event) = events.recv().await {
    ///     println!("{} from {}", event.command, event.origin);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[derive(Debug)]
    pub struct DaemonClient(Arc<DaemonClientInner>);

    struct DaemonClientInner {
        base_url: String,
        origin: String,
        sink: Mutex<Sink>,
        pending: Arc<Mutex<HashMap<String, oneshot::Sender<serde_json::Value>>>>,
        event_tx: broadcast::Sender<DaemonEvent>,
        timeout: Duration,
        reader_handle: Mutex<Option<JoinHandle<()>>>,
        connector: Connector,
        subscriptions: Mutex<Vec<String>>,
        connected: watch::Sender<bool>,
        max_reconnect_attempts: AtomicU32,
        disconnect_tx: broadcast::Sender<()>,
        reconnect_tx: broadcast::Sender<()>,
        /// Set by [`DaemonClient::close`], so that the client doesn't reconnect afterwards.
        closing: watch::Sender<bool>,
        closed: watch::Sender<bool>,
    }

    impl std::fmt::Debug for DaemonClientInner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("DaemonClientInner")
                .field("base_url", &self.base_url)
                .field("origin", &self.origin)
                .field("timeout", &self.timeout)
                .finish_non_exhaustive()
        }
    }

    impl DaemonClient {
        /// Connects to the Chia daemon over WSS, such as `wss://localhost:55400`.
        ///
        /// The `connector` should be built from TLS helpers in `chia-sdk-client`
        /// (e.g. `create_native_tls_connector` or `create_rustls_connector`)
        /// using the daemon's SSL certificate and key.
        ///
        /// The `timeout` applies to each request. Once connected, the client registers itself
        /// with the daemon under a unique origin name, so that responses are routed to it.
        ///
        /// # Panics
        ///
        /// Panics if called outside of a Tokio runtime, since it spawns the background task.
        pub async fn connect(
            url: &str,
            connector: Connector,
            timeout: Duration,
        ) -> Result<Self, DaemonError> {
            let (ws, _) = tokio_tungstenite::connect_async_tls_with_config(
                url,
                None,
                false,
                Some(connector.clone()),
            )
            .await?;

            let (sink, stream) = ws.split();

            let pending: Arc<Mutex<HashMap<String, oneshot::Sender<serde_json::Value>>>> =
                Arc::new(Mutex::new(HashMap::new()));
            let (event_tx, _) = broadcast::channel::<DaemonEvent>(256);

            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let origin = format!("chia-wallet-sdk-{nanos}");

            let (connected_tx, _) = watch::channel(true);
            let (disconnect_tx, _) = broadcast::channel::<()>(16);
            let (reconnect_tx, _) = broadcast::channel::<()>(16);
            let (closing_tx, _) = watch::channel(false);
            let (closed_tx, _) = watch::channel(false);

            let inner = Arc::new(DaemonClientInner {
                base_url: url.to_string(),
                origin,
                sink: Mutex::new(sink),
                pending,
                event_tx,
                timeout,
                reader_handle: Mutex::new(None),
                connector,
                subscriptions: Mutex::new(Vec::new()),
                connected: connected_tx,
                max_reconnect_attempts: AtomicU32::new(u32::MAX),
                disconnect_tx,
                reconnect_tx,
                closing: closing_tx,
                closed: closed_tx,
            });

            let reader_handle = Self::spawn_reader(inner.clone(), stream);

            *inner.reader_handle.lock().await = Some(reader_handle);

            let client = Self(inner);

            // Register own origin first, then track it for reconnection.
            // If this fails, clean up the reader task so we don't leak it.
            if let Err(e) = client.register_service(&client.0.origin.clone()).await {
                if let Some(handle) = client.0.reader_handle.lock().await.take() {
                    handle.abort();
                }
                return Err(e);
            }
            client
                .0
                .subscriptions
                .lock()
                .await
                .push(client.0.origin.clone());

            Ok(client)
        }

        /// Returns a receiver for daemon events (e.g. metrics, state changes).
        ///
        /// Calling `subscribe` also sends a `register_service` command to the daemon
        /// for the given service name, so that the daemon will forward events for
        /// that service to this client. Each service is only registered once, and is
        /// registered again after reconnecting.
        ///
        /// Every receiver gets every event, regardless of which service it was subscribed
        /// with. The receiver lags and skips events if more than 256 are waiting in it.
        pub async fn subscribe(
            &self,
            service: &str,
        ) -> Result<broadcast::Receiver<DaemonEvent>, DaemonError> {
            let mut subs = self.0.subscriptions.lock().await;
            if !subs.iter().any(|s| s == service) {
                self.register_service(service).await?;
                subs.push(service.to_string());
            }

            Ok(self.0.event_tx.subscribe())
        }

        async fn register_service(&self, service: &str) -> Result<(), DaemonError> {
            if !self.is_usable() {
                return Err(DaemonError::ConnectionClosed);
            }

            let request_id = next_request_id();

            let request = WebsocketRequest {
                command: "register_service".to_string(),
                ack: false,
                origin: self.0.origin.clone(),
                destination: "daemon".to_string(),
                request_id,
                data: serde_json::json!({ "service": service }),
            };

            let msg = serde_json::to_string(&request)?;
            self.0
                .sink
                .lock()
                .await
                .send(tungstenite::Message::Text(msg.into()))
                .await
                .map_err(|_| DaemonError::SendFailed)?;

            Ok(())
        }

        /// Closes the websocket connection.
        ///
        /// New requests and subscriptions fail with [`DaemonError::ConnectionClosed`] from now
        /// on. Once the daemon completes the close handshake, the background task stops
        /// without reconnecting. If the client is reconnecting, it stops trying instead.
        ///
        /// This is the only way to release the connection. Dropping every handle to the client
        /// doesn't stop the background task, which keeps the connection open and reconnects if
        /// it's lost.
        pub async fn close(&self) -> Result<(), DaemonError> {
            // This is set before the sink is locked, so a reconnection that installs a new sink
            // either sees it and closes that sink itself, or installs it before this closes it.
            self.0.closing.send_replace(true);

            let result = self.0.sink.lock().await.close().await;

            // While reconnecting, the sink belongs to the broken connection, so it can't be
            // closed cleanly. The reconnection stops on its own once it sees `closing`.
            if result.is_err() && !*self.0.connected.borrow() {
                return Ok(());
            }

            result?;
            Ok(())
        }

        /// Whether requests can be sent, because the client is connected and isn't closing.
        fn is_usable(&self) -> bool {
            *self.0.connected.borrow() && !*self.0.closing.borrow()
        }

        /// Returns a receiver that fires when the connection is lost unexpectedly, before the
        /// client tries to reconnect.
        pub fn on_disconnect(&self) -> broadcast::Receiver<()> {
            self.0.disconnect_tx.subscribe()
        }

        /// Returns a receiver that fires when the connection is re-established after a disconnect.
        pub fn on_reconnect(&self) -> broadcast::Receiver<()> {
            self.0.reconnect_tx.subscribe()
        }

        /// Waits until the client has stopped for good, because the connection was closed
        /// cleanly (by [`DaemonClient::close`] or by the daemon) or reconnecting gave up.
        ///
        /// Once this happens, requests fail with [`DaemonError::ConnectionClosed`], and no more
        /// events are received.
        pub async fn closed(&self) {
            let mut closed = self.0.closed.subscribe();
            closed.wait_for(|closed| *closed).await.ok();
        }

        /// Sets the maximum number of reconnection attempts before giving up.
        /// `None` means unlimited (the default).
        pub fn set_max_reconnect_attempts(&self, max: Option<u32>) {
            self.0
                .max_reconnect_attempts
                .store(max.unwrap_or(u32::MAX), Ordering::Relaxed);
        }

        async fn send_request<R>(
            &self,
            command: &str,
            destination: &str,
            data: serde_json::Value,
        ) -> Result<R, DaemonError>
        where
            R: DeserializeOwned + Send,
        {
            if !self.is_usable() {
                return Err(DaemonError::ConnectionClosed);
            }

            let request_id = next_request_id();

            let request = WebsocketRequest {
                command: command.to_string(),
                ack: false,
                origin: self.0.origin.clone(),
                destination: destination.to_string(),
                request_id: request_id.clone(),
                data,
            };

            let (tx, rx) = oneshot::channel();
            self.0.pending.lock().await.insert(request_id.clone(), tx);

            let msg = serde_json::to_string(&request)?;
            self.0
                .sink
                .lock()
                .await
                .send(tungstenite::Message::Text(msg.into()))
                .await
                .map_err(|_| DaemonError::SendFailed)?;

            let value = tokio::time::timeout(self.0.timeout, rx)
                .await
                .map_err(|_| {
                    // Timed out -- clean up the pending entry so the reader
                    // doesn't try to send to a closed channel later.
                    let pending = self.0.pending.clone();
                    let id = request_id.clone();
                    tokio::spawn(async move { pending.lock().await.remove(&id) });
                    DaemonError::Timeout(self.0.timeout)
                })?
                .map_err(|_| DaemonError::ReceiveFailed)?;

            Ok(serde_json::from_value(value)?)
        }

        /// Spawns the task that reads messages from the daemon. If the connection fails, the
        /// task reconnects, and if it closes cleanly or reconnecting gives up, the client is
        /// marked as closed.
        fn spawn_reader(inner: Arc<DaemonClientInner>, stream: Stream) -> JoinHandle<()> {
            tokio::spawn(async move {
                match handle_inbound_messages(stream, inner.pending.clone(), inner.event_tx.clone())
                    .await
                {
                    Ok(()) => {
                        debug!("Daemon connection closed");
                        // If the daemon started the close, this sends the reply that tungstenite
                        // queued, which completes the handshake and ends the connection.
                        inner.sink.lock().await.close().await.ok();
                    }
                    Err(error) if *inner.closing.borrow() => {
                        debug!("Daemon reader task ended after closing: {error}");
                    }
                    Err(error) => {
                        debug!("Daemon reader task ended: {error}");
                        match Self::reconnect(inner.clone()).await {
                            Ok(()) => return,
                            Err(DaemonError::ConnectionClosed) => {
                                debug!("Stopped reconnecting because the client was closed");
                            }
                            Err(e) => error!("Reconnection failed permanently: {e}"),
                        }
                    }
                }

                inner.connected.send_replace(false);
                inner.pending.lock().await.clear();
                inner.closed.send_replace(true);
            })
        }

        /// Breaks the recursive async cycle by boxing the future. The actual
        /// reconnection logic lives in `reconnect_inner`.
        fn reconnect(
            inner: Arc<DaemonClientInner>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), DaemonError>> + Send>>
        {
            Box::pin(Self::reconnect_inner(inner))
        }

        async fn reconnect_inner(inner: Arc<DaemonClientInner>) -> Result<(), DaemonError> {
            inner.connected.send_modify(|val| *val = false);

            // Drain all pending requests so callers get immediate ReceiveFailed
            // instead of hanging until their individual timeouts expire.
            {
                let mut pending = inner.pending.lock().await;
                for (_, sender) in pending.drain() {
                    drop(sender);
                }
            }

            inner.disconnect_tx.send(()).ok();

            let mut attempt = 0u32;
            let mut backoff = Duration::from_secs(1);
            let max_backoff = Duration::from_secs(30);
            let mut closing = inner.closing.subscribe();

            loop {
                if *closing.borrow() {
                    return Err(DaemonError::ConnectionClosed);
                }

                attempt += 1;
                let max = inner.max_reconnect_attempts.load(Ordering::Relaxed);
                if max != u32::MAX && attempt > max {
                    return Err(DaemonError::ReconnectFailed(max));
                }

                info!("Reconnecting to daemon (attempt {attempt})...");

                let connect = tokio_tungstenite::connect_async_tls_with_config(
                    &inner.base_url,
                    None,
                    false,
                    Some(inner.connector.clone()),
                );

                let result = tokio::select! {
                    result = connect => result,
                    _ = closing.wait_for(|closing| *closing) => {
                        return Err(DaemonError::ConnectionClosed);
                    }
                };

                match result {
                    Ok((ws, _)) => {
                        let (mut new_sink, new_stream) = ws.split();

                        // The services are cloned first, since `subscribe` locks them before
                        // the sink. A service subscribed while reconnecting fails to register,
                        // so it isn't missed here.
                        let subs = inner.subscriptions.lock().await.clone();

                        {
                            let mut sink = inner.sink.lock().await;

                            // `close` sets this before it locks the sink, so if it isn't set
                            // here, `close` will close the new sink once this unlocks it.
                            if *inner.closing.borrow() {
                                new_sink.close().await.ok();
                                return Err(DaemonError::ConnectionClosed);
                            }

                            // Re-register all subscriptions (origin is first in the vec)
                            for service in &subs {
                                let request_id = next_request_id();
                                let request = WebsocketRequest {
                                    command: "register_service".to_string(),
                                    ack: false,
                                    origin: inner.origin.clone(),
                                    destination: "daemon".to_string(),
                                    request_id,
                                    data: serde_json::json!({ "service": service }),
                                };
                                if let Ok(msg) = serde_json::to_string(&request) {
                                    new_sink
                                        .send(tungstenite::Message::Text(msg.into()))
                                        .await
                                        .ok();
                                }
                            }

                            *sink = new_sink;

                            // This is set before the new reader starts, so that if the new
                            // connection ends at once, the reader marks it as disconnected
                            // after this, rather than this marking it as connected again.
                            inner.connected.send_replace(true);
                        }

                        // This runs on the old reader task, after its stream ended, so the
                        // old handle is replaced without aborting it. Aborting would cancel
                        // this task at its next await.
                        let new_handle = Self::spawn_reader(inner.clone(), new_stream);
                        *inner.reader_handle.lock().await = Some(new_handle);

                        inner.reconnect_tx.send(()).ok();

                        info!("Reconnected to daemon successfully");
                        return Ok(());
                    }
                    Err(e) => {
                        warn!("Reconnect attempt {attempt} failed: {e}");
                        tokio::select! {
                            () = tokio::time::sleep(backoff) => {}
                            _ = closing.wait_for(|closing| *closing) => {
                                return Err(DaemonError::ConnectionClosed);
                            }
                        }
                        backoff = (backoff * 2).min(max_backoff);
                    }
                }
            }
        }
    }

    impl Clone for DaemonClient {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl ChiaRpcClient for DaemonClient {
        type Error = DaemonError;

        fn base_url(&self) -> &str {
            &self.0.base_url
        }

        async fn make_post_request<R, B>(&self, endpoint: &str, body: B) -> Result<R, Self::Error>
        where
            B: Serialize + Send,
            R: DeserializeOwned + Send,
        {
            let data = serde_json::to_value(body)?;
            self.send_request(endpoint, "chia_full_node", data).await
        }
    }

    impl Drop for DaemonClientInner {
        fn drop(&mut self) {
            if let Some(handle) = self.reader_handle.get_mut().take() {
                handle.abort();
            }
        }
    }

    async fn handle_inbound_messages(
        mut stream: Stream,
        pending: Arc<Mutex<HashMap<String, oneshot::Sender<serde_json::Value>>>>,
        event_tx: broadcast::Sender<DaemonEvent>,
    ) -> Result<(), DaemonError> {
        while let Some(message) = stream.next().await {
            let message = message?;

            let text = match message {
                tungstenite::Message::Text(text) => text,
                tungstenite::Message::Binary(bin) => tungstenite::Utf8Bytes::try_from(bin)
                    .map_err(|_| DaemonError::ConnectionClosed)?,
                tungstenite::Message::Close(..) => break,
                tungstenite::Message::Ping(..)
                | tungstenite::Message::Pong(..)
                | tungstenite::Message::Frame(..) => continue,
            };

            let response: WebsocketResponse = match serde_json::from_str(&text) {
                Ok(resp) => resp,
                Err(e) => {
                    warn!("Failed to parse daemon message: {e}");
                    continue;
                }
            };

            let mut pending_guard = pending.lock().await;
            if let Some(sender) = pending_guard.remove(&response.request_id) {
                drop(pending_guard);
                sender.send(response.data).ok();
            } else {
                drop(pending_guard);

                let event = DaemonEvent {
                    command: response.command,
                    origin: response.origin,
                    data: response.data,
                };

                event_tx.send(event).ok();
            }
        }

        Ok(())
    }
}

#[cfg(any(feature = "native-tls", feature = "rustls"))]
pub use inner::DaemonClient;
