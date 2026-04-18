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
        /// Connects to the Chia daemon over WSS.
        ///
        /// The `connector` should be built from TLS helpers in `chia-sdk-client`
        /// (e.g. `create_native_tls_connector` or `create_rustls_connector`)
        /// using the daemon's SSL certificate and key.
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
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let origin = format!("chia-wallet-sdk-{nanos}");

            let (connected_tx, _) = watch::channel(true);
            let (disconnect_tx, _) = broadcast::channel::<()>(16);
            let (reconnect_tx, _) = broadcast::channel::<()>(16);

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
            });

            let pending_clone = inner.pending.clone();
            let event_tx_clone = inner.event_tx.clone();
            let inner_for_reader = inner.clone();

            let reader_handle = tokio::spawn(async move {
                if let Err(error) =
                    handle_inbound_messages(stream, pending_clone, event_tx_clone).await
                {
                    debug!("Daemon reader task ended: {error}");
                    if let Err(e) = Self::reconnect(inner_for_reader).await {
                        error!("Reconnection failed permanently: {e}");
                    }
                }
            });

            *inner.reader_handle.lock().await = Some(reader_handle);

            let client = Self(inner);

            // Register own origin first, then track it for reconnection
            client.register_service(&client.0.origin.clone()).await?;
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
        /// that service to this client.
        pub async fn subscribe(
            &self,
            service: &str,
        ) -> Result<broadcast::Receiver<DaemonEvent>, DaemonError> {
            self.register_service(service).await?;

            let mut subs = self.0.subscriptions.lock().await;
            if !subs.iter().any(|s| s == service) {
                subs.push(service.to_string());
            }

            Ok(self.0.event_tx.subscribe())
        }

        async fn register_service(&self, service: &str) -> Result<(), DaemonError> {
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
                .send(tungstenite::Message::Text(msg))
                .await
                .map_err(|_| DaemonError::SendFailed)?;

            Ok(())
        }

        /// Closes the websocket connection and aborts the background reader task.
        pub async fn close(&self) -> Result<(), DaemonError> {
            self.0.sink.lock().await.close().await?;
            Ok(())
        }

        /// Returns a receiver that fires when the connection is lost.
        pub fn on_disconnect(&self) -> broadcast::Receiver<()> {
            self.0.disconnect_tx.subscribe()
        }

        /// Returns a receiver that fires when the connection is re-established after a disconnect.
        pub fn on_reconnect(&self) -> broadcast::Receiver<()> {
            self.0.reconnect_tx.subscribe()
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
            if !*self.0.connected.subscribe().borrow() {
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
                .send(tungstenite::Message::Text(msg))
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

        /// Breaks the recursive async cycle by boxing the future. The actual
        /// reconnection logic lives in `reconnect_inner`.
        fn reconnect(
            inner: Arc<DaemonClientInner>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), DaemonError>> + Send>>
        {
            Box::pin(Self::reconnect_inner(inner))
        }

        async fn reconnect_inner(inner: Arc<DaemonClientInner>) -> Result<(), DaemonError> {
            inner.connected.send(false).ok();

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

            loop {
                attempt += 1;
                let max = inner.max_reconnect_attempts.load(Ordering::Relaxed);
                if max != u32::MAX && attempt > max {
                    return Err(DaemonError::ReconnectFailed(max));
                }

                info!("Reconnecting to daemon (attempt {attempt})...");

                match tokio_tungstenite::connect_async_tls_with_config(
                    &inner.base_url,
                    None,
                    false,
                    Some(inner.connector.clone()),
                )
                .await
                {
                    Ok((ws, _)) => {
                        let (new_sink, new_stream) = ws.split();

                        *inner.sink.lock().await = new_sink;

                        // Abort old reader task if still running, then spawn a new one
                        if let Some(handle) = inner.reader_handle.lock().await.take() {
                            handle.abort();
                        }

                        let pending_clone = inner.pending.clone();
                        let event_tx_clone = inner.event_tx.clone();
                        let inner_clone = inner.clone();
                        let new_handle = tokio::spawn(async move {
                            if let Err(error) =
                                handle_inbound_messages(new_stream, pending_clone, event_tx_clone)
                                    .await
                            {
                                debug!("Daemon reader task ended: {error}");
                                if let Err(e) = Self::reconnect(inner_clone).await {
                                    error!("Reconnection failed permanently: {e}");
                                }
                            }
                        });
                        *inner.reader_handle.lock().await = Some(new_handle);

                        // Re-register all subscriptions (origin is first in the vec)
                        let subs = inner.subscriptions.lock().await.clone();
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
                                inner
                                    .sink
                                    .lock()
                                    .await
                                    .send(tungstenite::Message::Text(msg))
                                    .await
                                    .ok();
                            }
                        }

                        inner.connected.send(true).ok();
                        inner.reconnect_tx.send(()).ok();

                        info!("Reconnected to daemon successfully");
                        return Ok(());
                    }
                    Err(e) => {
                        warn!("Reconnect attempt {attempt} failed: {e}");
                        tokio::time::sleep(backoff).await;
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
                tungstenite::Message::Binary(bin) => {
                    String::from_utf8(bin).map_err(|_| DaemonError::ConnectionClosed)?
                }
                tungstenite::Message::Close(..) => break,
                tungstenite::Message::Ping(..) | tungstenite::Message::Pong(..) => continue,
                _ => continue,
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
