use std::{sync::Arc, time::Duration};

use bindy::{Error, Result};
use chia_sdk_daemon::DaemonClient as SdkDaemonClient;
use tokio::sync::{
    Mutex,
    broadcast::{Receiver, error::RecvError},
};

use crate::{Connector, RpcClient};

/// A websocket client for the Chia daemon, which can send full node RPC requests through the
/// daemon and receive the events that services broadcast.
#[derive(Clone)]
pub struct DaemonClient {
    client: SdkDaemonClient,
    disconnects: Arc<Mutex<Receiver<()>>>,
    reconnects: Arc<Mutex<Receiver<()>>>,
}

impl DaemonClient {
    /// Connects to the daemon over WSS, such as `wss://localhost:55400`, authenticating with the
    /// connector's certificate. Each request fails if the daemon doesn't respond within
    /// `timeout_seconds`.
    pub async fn connect(url: String, connector: Connector, timeout_seconds: f64) -> Result<Self> {
        let timeout = Duration::try_from_secs_f64(timeout_seconds)
            .map_err(|error| Error::Custom(format!("Invalid timeout: {error}")))?;

        let client = SdkDaemonClient::connect(&url, connector.0, timeout).await?;

        Ok(Self {
            disconnects: Arc::new(Mutex::new(client.on_disconnect())),
            reconnects: Arc::new(Mutex::new(client.on_reconnect())),
            client,
        })
    }

    /// Returns an RPC client that sends full node requests through this connection.
    pub fn rpc_client(&self) -> Result<RpcClient> {
        Ok(RpcClient::from_daemon(self.client.clone()))
    }

    /// Registers the service with the daemon, so that its events are forwarded to this client,
    /// and returns a subscription that receives every event from now on.
    pub async fn subscribe(&self, service: String) -> Result<DaemonSubscription> {
        let receiver = self.client.subscribe(&service).await?;
        Ok(DaemonSubscription {
            client: self.client.clone(),
            receiver: Arc::new(Mutex::new(receiver)),
        })
    }

    /// Closes the connection. The client doesn't reconnect afterwards.
    pub async fn close(&self) -> Result<()> {
        Ok(self.client.close().await?)
    }

    /// Sets the maximum number of reconnection attempts before giving up, or `None` for no limit.
    pub fn set_max_reconnect_attempts(&self, max_attempts: Option<u32>) -> Result<()> {
        self.client.set_max_reconnect_attempts(max_attempts);
        Ok(())
    }

    /// Waits for the next unexpected disconnect since connecting or the previous call. Returns
    /// `false` instead if the client has stopped for good.
    pub async fn next_disconnect(&self) -> Result<bool> {
        Ok(next_signal(&self.client, &self.disconnects).await)
    }

    /// Waits for the next successful reconnection since connecting or the previous call. Returns
    /// `false` instead if the client has stopped for good.
    pub async fn next_reconnect(&self) -> Result<bool> {
        Ok(next_signal(&self.client, &self.reconnects).await)
    }
}

async fn next_signal(client: &SdkDaemonClient, receiver: &Mutex<Receiver<()>>) -> bool {
    let mut receiver = receiver.lock().await;

    tokio::select! {
        biased;
        // A lagged receiver still means that at least one signal was sent.
        result = receiver.recv() => !matches!(result, Err(RecvError::Closed)),
        () = client.closed() => false,
    }
}

/// Receives events from the daemon, returned by [`DaemonClient::subscribe`].
#[derive(Clone)]
pub struct DaemonSubscription {
    client: SdkDaemonClient,
    receiver: Arc<Mutex<Receiver<chia_sdk_daemon::DaemonEvent>>>,
}

impl DaemonSubscription {
    /// Waits for the next event, or returns `None` once the client has stopped for good and
    /// every event received before then has been returned.
    ///
    /// If more than 256 events were waiting, the oldest are skipped and this fails with an error
    /// saying how many. The subscription can still be used afterwards.
    pub async fn next(&self) -> Result<Option<DaemonEvent>> {
        let mut receiver = self.receiver.lock().await;

        let result = tokio::select! {
            biased;
            result = receiver.recv() => result,
            () = self.client.closed() => return Ok(receiver.try_recv().ok().map(DaemonEvent::from)),
        };

        match result {
            Ok(event) => Ok(Some(event.into())),
            Err(RecvError::Closed) => Ok(None),
            Err(RecvError::Lagged(skipped)) => Err(Error::Custom(format!(
                "Daemon subscription fell behind and skipped {skipped} events"
            ))),
        }
    }
}

/// A message from the daemon that wasn't a response to a request, such as a state change or
/// metrics event from a service.
#[derive(Clone)]
pub struct DaemonEvent {
    /// The command or event name.
    pub command: String,
    /// The service that sent the event.
    pub origin: String,
    /// The event's data, as a JSON string.
    pub data: String,
}

impl From<chia_sdk_daemon::DaemonEvent> for DaemonEvent {
    fn from(event: chia_sdk_daemon::DaemonEvent) -> Self {
        Self {
            command: event.command,
            origin: event.origin,
            data: event.data.to_string(),
        }
    }
}
