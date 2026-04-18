use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
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
    sync::{Mutex, broadcast, oneshot},
    task::JoinHandle,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, warn};

#[cfg(any(feature = "native-tls", feature = "rustls"))]
use tokio_tungstenite::Connector;

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

#[derive(Debug)]
struct DaemonClientInner {
    base_url: String,
    origin: String,
    sink: Mutex<Sink>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<serde_json::Value>>>>,
    event_tx: broadcast::Sender<DaemonEvent>,
    timeout: Duration,
    reader_handle: JoinHandle<()>,
}

impl DaemonClient {
    /// Connects to the Chia daemon over WSS.
    ///
    /// The `connector` should be built from TLS helpers in `chia-sdk-client`
    /// (e.g. `create_native_tls_connector` or `create_rustls_connector`)
    /// using the daemon's SSL certificate and key.
    #[cfg(any(feature = "native-tls", feature = "rustls"))]
    pub async fn connect(
        url: &str,
        connector: Connector,
        timeout: Duration,
    ) -> Result<Self, DaemonError> {
        let (ws, _) =
            tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(connector))
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

        let pending_clone = pending.clone();
        let event_tx_clone = event_tx.clone();

        let reader_handle = tokio::spawn(async move {
            if let Err(error) = handle_inbound_messages(stream, pending_clone, event_tx_clone).await
            {
                debug!("Daemon reader task ended: {error}");
            }
        });

        let client = Self(Arc::new(DaemonClientInner {
            base_url: url.to_string(),
            origin,
            sink: Mutex::new(sink),
            pending,
            event_tx,
            timeout,
            reader_handle,
        }));

        client.register_service(&client.0.origin.clone()).await?;

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
        self.reader_handle.abort();
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
