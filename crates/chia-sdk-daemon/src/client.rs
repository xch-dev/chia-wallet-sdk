#[cfg(any(feature = "native-tls", feature = "rustls"))]
mod inner {
    use std::{
        collections::HashMap,
        sync::{
            Arc, Weak,
            atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
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
    type Pending = std::sync::Mutex<HashMap<String, oneshot::Sender<serde_json::Value>>>;

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
        pending: Arc<Pending>,
        event_tx: broadcast::Sender<DaemonEvent>,
        timeout: Duration,
        reader_handle: std::sync::Mutex<Option<JoinHandle<()>>>,
        connector: Connector,
        subscriptions: Mutex<Vec<String>>,
        connected: watch::Sender<bool>,
        closed: AtomicBool,
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

    impl DaemonClientInner {
        /// Marks the connection as down and fails all pending requests immediately,
        /// so that callers don't wait for their individual timeouts to expire.
        fn mark_disconnected(&self) {
            // This must happen before draining, so that a request which is inserted
            // after the drain sees that the connection is down.
            self.connected.send_replace(false);

            let pending = std::mem::take(&mut *self.pending.lock().expect("pending poisoned"));
            drop(pending);
        }

        /// Marks the connection as up, unless the client has been closed in the meantime.
        fn mark_connected(&self) -> bool {
            self.connected.send_if_modified(|connected| {
                if self.closed.load(Ordering::SeqCst) {
                    return false;
                }
                *connected = true;
                true
            })
        }

        async fn register_service(&self, service: &str) -> Result<(), DaemonError> {
            let request = WebsocketRequest {
                command: "register_service".to_string(),
                ack: false,
                origin: self.origin.clone(),
                destination: "daemon".to_string(),
                request_id: next_request_id(),
                data: serde_json::json!({ "service": service }),
            };

            let msg = serde_json::to_string(&request)?;
            self.sink
                .lock()
                .await
                .send(tungstenite::Message::Text(msg.into()))
                .await
                .map_err(|_| DaemonError::SendFailed)?;

            Ok(())
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

            let (event_tx, _) = broadcast::channel::<DaemonEvent>(256);

            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let origin = format!("chia-wallet-sdk-{nanos}");

            let (connected_tx, _) = watch::channel(true);
            let (disconnect_tx, _) = broadcast::channel::<()>(16);
            let (reconnect_tx, _) = broadcast::channel::<()>(16);

            let inner = Arc::new(DaemonClientInner {
                base_url: url.to_string(),
                origin,
                sink: Mutex::new(sink),
                pending: Arc::new(std::sync::Mutex::new(HashMap::new())),
                event_tx,
                timeout,
                reader_handle: std::sync::Mutex::new(None),
                connector,
                subscriptions: Mutex::new(Vec::new()),
                connected: connected_tx,
                closed: AtomicBool::new(false),
                max_reconnect_attempts: AtomicU32::new(u32::MAX),
                disconnect_tx,
                reconnect_tx,
            });

            // The reader only holds a weak reference, so that dropping the last client
            // drops the inner state, which in turn aborts the reader.
            let reader_handle = tokio::spawn(run_reader(
                Arc::downgrade(&inner),
                stream,
                inner.pending.clone(),
                inner.event_tx.clone(),
            ));

            *inner.reader_handle.lock().expect("reader handle poisoned") = Some(reader_handle);

            let client = Self(inner);

            // Register own origin first, then track it for reconnection.
            // If this fails, dropping the client cleans up the reader task.
            client.0.register_service(&client.0.origin).await?;
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
            let mut subs = self.0.subscriptions.lock().await;
            if !subs.iter().any(|s| s == service) {
                self.0.register_service(service).await?;
                subs.push(service.to_string());
            }

            Ok(self.0.event_tx.subscribe())
        }

        /// Closes the websocket connection and aborts the background reader task.
        /// The client will not reconnect after this, and any further requests will fail.
        pub async fn close(&self) -> Result<(), DaemonError> {
            self.0.closed.store(true, Ordering::SeqCst);

            if let Some(handle) = self
                .0
                .reader_handle
                .lock()
                .expect("reader handle poisoned")
                .take()
            {
                handle.abort();
            }

            self.0.mark_disconnected();

            let close = async { self.0.sink.lock().await.close().await };

            match tokio::time::timeout(self.0.timeout, close).await {
                Ok(
                    Ok(())
                    | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed),
                ) => Ok(()),
                Ok(Err(error)) => Err(error.into()),
                Err(_) => Err(DaemonError::Timeout(self.0.timeout)),
            }
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
            let request_id = next_request_id();

            let request = WebsocketRequest {
                command: command.to_string(),
                ack: false,
                origin: self.0.origin.clone(),
                destination: destination.to_string(),
                request_id: request_id.clone(),
                data,
            };

            let msg = serde_json::to_string(&request)?;

            let (tx, rx) = oneshot::channel();
            self.0
                .pending
                .lock()
                .expect("pending poisoned")
                .insert(request_id.clone(), tx);
            let _pending = PendingRequest {
                pending: &self.0.pending,
                request_id: &request_id,
            };

            // This is checked after inserting the request, so that a disconnect
            // either drains the request or is observed here.
            if !*self.0.connected.borrow() {
                return Err(DaemonError::ConnectionClosed);
            }

            self.0
                .sink
                .lock()
                .await
                .send(tungstenite::Message::Text(msg.into()))
                .await
                .map_err(|_| DaemonError::SendFailed)?;

            let value = tokio::time::timeout(self.0.timeout, rx)
                .await
                .map_err(|_| DaemonError::Timeout(self.0.timeout))?
                .map_err(|_| DaemonError::ReceiveFailed)?;

            Ok(serde_json::from_value(value)?)
        }
    }

    /// Removes the request from the pending map when it completes, fails, or is cancelled.
    struct PendingRequest<'a> {
        pending: &'a Pending,
        request_id: &'a str,
    }

    impl Drop for PendingRequest<'_> {
        fn drop(&mut self) {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(self.request_id);
            }
        }
    }

    /// Reads messages until the connection is lost, then reconnects and continues
    /// with the new connection. This runs as a single task for the lifetime of the
    /// client, so it never has to abort or replace itself.
    async fn run_reader(
        inner: Weak<DaemonClientInner>,
        mut stream: Stream,
        pending: Arc<Pending>,
        event_tx: broadcast::Sender<DaemonEvent>,
    ) {
        loop {
            match handle_inbound_messages(stream, &pending, &event_tx).await {
                Ok(()) => debug!("Daemon closed the connection"),
                Err(error) => debug!("Daemon reader task ended: {error}"),
            }

            let Some(strong) = inner.upgrade() else {
                return;
            };

            strong.mark_disconnected();

            if strong.closed.load(Ordering::SeqCst) {
                return;
            }

            strong.disconnect_tx.send(()).ok();
            drop(strong);

            match reconnect(&inner).await {
                Ok(new_stream) => stream = new_stream,
                Err(error) => {
                    error!("Reconnection failed permanently: {error}");
                    return;
                }
            }
        }
    }

    async fn reconnect(inner: &Weak<DaemonClientInner>) -> Result<Stream, DaemonError> {
        let mut attempt = 0u32;
        let mut backoff = Duration::from_secs(1);
        let max_backoff = Duration::from_secs(30);

        loop {
            attempt += 1;

            // Don't hold a strong reference while waiting, so the client can still be dropped.
            let (base_url, connector) = {
                let Some(inner) = inner.upgrade() else {
                    return Err(DaemonError::ConnectionClosed);
                };

                if inner.closed.load(Ordering::SeqCst) {
                    return Err(DaemonError::ConnectionClosed);
                }

                let max = inner.max_reconnect_attempts.load(Ordering::Relaxed);
                if max != u32::MAX && attempt > max {
                    return Err(DaemonError::ReconnectFailed(max));
                }

                (inner.base_url.clone(), inner.connector.clone())
            };

            info!("Reconnecting to daemon (attempt {attempt})...");

            match tokio_tungstenite::connect_async_tls_with_config(
                &base_url,
                None,
                false,
                Some(connector),
            )
            .await
            {
                Ok((ws, _)) => {
                    let Some(inner) = inner.upgrade() else {
                        return Err(DaemonError::ConnectionClosed);
                    };

                    let (new_sink, new_stream) = ws.split();
                    *inner.sink.lock().await = new_sink;

                    // Re-register all subscriptions (origin is first in the vec)
                    let subs = inner.subscriptions.lock().await.clone();
                    for service in &subs {
                        if let Err(error) = inner.register_service(service).await {
                            warn!("Failed to re-register service {service}: {error}");
                        }
                    }

                    if !inner.mark_connected() {
                        return Err(DaemonError::ConnectionClosed);
                    }

                    inner.reconnect_tx.send(()).ok();

                    info!("Reconnected to daemon successfully");
                    return Ok(new_stream);
                }
                Err(e) => {
                    warn!("Reconnect attempt {attempt} failed: {e}");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(max_backoff);
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
            if let Some(handle) = self.reader_handle.get_mut().ok().and_then(Option::take) {
                handle.abort();
            }
        }
    }

    async fn handle_inbound_messages(
        mut stream: Stream,
        pending: &Pending,
        event_tx: &broadcast::Sender<DaemonEvent>,
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

            let sender = pending
                .lock()
                .expect("pending poisoned")
                .remove(&response.request_id);

            if let Some(sender) = sender {
                sender.send(response.data).ok();
            } else {
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

    #[cfg(test)]
    mod tests {
        use tokio::net::TcpListener;
        use tokio_tungstenite::accept_async;

        use super::*;

        type ServerSocket = WebSocketStream<TcpStream>;

        async fn accept(listener: &TcpListener) -> anyhow::Result<ServerSocket> {
            let (stream, _) = listener.accept().await?;
            Ok(accept_async(stream).await?)
        }

        async fn connect(
            timeout: Duration,
        ) -> anyhow::Result<(DaemonClient, ServerSocket, TcpListener)> {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let url = format!("ws://{}", listener.local_addr()?);
            let (client, server) = tokio::join!(
                DaemonClient::connect(&url, Connector::Plain, timeout),
                accept(&listener)
            );
            Ok((client?, server?, listener))
        }

        /// Reads requests until one with the given command arrives.
        async fn recv_request(
            server: &mut ServerSocket,
            command: &str,
        ) -> anyhow::Result<serde_json::Value> {
            loop {
                let message = server.next().await.expect("connection closed")?;
                if let tungstenite::Message::Text(text) = message {
                    let request: serde_json::Value = serde_json::from_str(&text)?;
                    if request["command"] == command {
                        return Ok(request);
                    }
                }
            }
        }

        async fn respond(
            server: &mut ServerSocket,
            request: &serde_json::Value,
            data: serde_json::Value,
        ) -> anyhow::Result<()> {
            let response = serde_json::json!({
                "command": request["command"],
                "ack": true,
                "origin": request["destination"],
                "destination": request["origin"],
                "request_id": request["request_id"],
                "data": data,
            });
            server
                .send(tungstenite::Message::Text(response.to_string().into()))
                .await?;
            Ok(())
        }

        /// Fails the test instead of hanging if the future never completes.
        async fn within<F: Future>(future: F) -> F::Output {
            tokio::time::timeout(Duration::from_secs(10), future)
                .await
                .expect("future hung")
        }

        #[tokio::test]
        async fn test_drop_releases_connection() -> anyhow::Result<()> {
            let (client, mut server, _listener) = connect(Duration::from_secs(5)).await?;

            let weak = Arc::downgrade(&client.0);
            drop(client);
            assert!(weak.upgrade().is_none());

            // The reader task is aborted, so the connection is closed.
            within(async { while let Some(Ok(_)) = server.next().await {} }).await;

            Ok(())
        }

        #[tokio::test]
        async fn test_graceful_close_reconnects() -> anyhow::Result<()> {
            // The request timeout is long, so a fast failure shows that pending requests are drained.
            let (client, mut server, listener) = connect(Duration::from_secs(60)).await?;
            let mut disconnects = client.on_disconnect();
            let mut reconnects = client.on_reconnect();

            let request = tokio::spawn({
                let client = client.clone();
                async move {
                    client
                        .make_post_request::<serde_json::Value, _>("ping", serde_json::json!({}))
                        .await
                }
            });

            recv_request(&mut server, "ping").await?;
            server.close(None).await?;

            within(disconnects.recv()).await?;
            let result = within(request).await?;
            assert!(matches!(result, Err(DaemonError::ReceiveFailed)));

            let mut server = within(accept(&listener)).await?;
            within(reconnects.recv()).await?;

            // Subscriptions are re-registered and requests work again.
            recv_request(&mut server, "register_service").await?;

            let request = tokio::spawn({
                let client = client.clone();
                async move {
                    client
                        .make_post_request::<serde_json::Value, _>("ping", serde_json::json!({}))
                        .await
                }
            });

            let ping = recv_request(&mut server, "ping").await?;
            respond(&mut server, &ping, serde_json::json!({ "success": true })).await?;

            let response = within(request).await??;
            assert_eq!(response["success"], true);

            Ok(())
        }

        #[tokio::test]
        async fn test_close_does_not_reconnect() -> anyhow::Result<()> {
            let (client, mut server, listener) = connect(Duration::from_secs(5)).await?;

            within(client.close()).await?;
            assert!(client.0.reader_handle.lock().expect("poisoned").is_none());
            assert!(!*client.0.connected.borrow());

            within(async { while let Some(Ok(_)) = server.next().await {} }).await;

            // No reconnection attempt is made.
            assert!(
                tokio::time::timeout(Duration::from_secs(2), listener.accept())
                    .await
                    .is_err()
            );

            let result = within(
                client.make_post_request::<serde_json::Value, _>("ping", serde_json::json!({})),
            )
            .await;
            assert!(matches!(result, Err(DaemonError::ConnectionClosed)));

            Ok(())
        }
    }
}

#[cfg(any(feature = "native-tls", feature = "rustls"))]
pub use inner::DaemonClient;
