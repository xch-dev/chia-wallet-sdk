//! Tests `DaemonClient` against a fake daemon, a local websocket server that's driven by each
//! test, since there's no real daemon to connect to in CI.
//!
//! The fake daemon serves plain `ws://`, since tokio-tungstenite doesn't use the connector for
//! those URLs. The Node.js binding tests cover a TLS connection end to end.

// `DaemonClient` only exists with a TLS feature enabled.
#![cfg(any(feature = "native-tls", feature = "rustls"))]

use std::{future::Future, net::SocketAddr, time::Duration};

use chia_sdk_coinset::ChiaRpcClient;
use chia_sdk_daemon::{DaemonClient, DaemonError};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{Connector, WebSocketStream};
use tungstenite::Message;

type ServerSocket = WebSocketStream<TcpStream>;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const TIMEOUT: Duration = Duration::from_secs(10);

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(TIMEOUT, future)
        .await
        .expect("timed out")
}

struct FakeDaemon {
    listener: TcpListener,
}

impl FakeDaemon {
    async fn start() -> Result<Self> {
        Self::bind("127.0.0.1:0".parse()?).await
    }

    async fn bind(addr: SocketAddr) -> Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self { listener })
    }

    fn addr(&self) -> SocketAddr {
        self.listener.local_addr().unwrap()
    }

    fn url(&self) -> String {
        format!("ws://{}", self.addr())
    }

    async fn accept(&self) -> ServerSocket {
        let (tcp, _) = self.listener.accept().await.unwrap();
        tokio_tungstenite::accept_async(tcp).await.unwrap()
    }
}

async fn recv_json(ws: &mut ServerSocket) -> Value {
    loop {
        match ws.next().await.expect("stream ended").unwrap() {
            Message::Text(text) => return serde_json::from_str(&text).unwrap(),
            Message::Ping(..) | Message::Pong(..) => {}
            message => panic!("unexpected message: {message:?}"),
        }
    }
}

async fn send_json(ws: &mut ServerSocket, value: &Value) {
    ws.send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}

/// Reads a `register_service` request and returns the registered service name.
async fn expect_registration(ws: &mut ServerSocket, origin: Option<&str>) -> String {
    let request = recv_json(ws).await;
    assert_eq!(request["command"], "register_service");
    assert_eq!(request["destination"], "daemon");
    if let Some(origin) = origin {
        assert_eq!(request["origin"], origin);
    }
    request["data"]["service"].as_str().unwrap().to_string()
}

/// Accepts a connection and reads the client's registration of its own origin.
async fn accept_client(daemon: &FakeDaemon) -> (DaemonClient, ServerSocket, String) {
    let url = daemon.url();
    let connect = Box::pin(DaemonClient::connect(&url, Connector::Plain, TIMEOUT));
    let (client, mut ws) = within(async { tokio::join!(connect, daemon.accept()) }).await;
    let client = client.unwrap();

    let origin = within(expect_registration(&mut ws, None)).await;
    assert!(origin.starts_with("chia-wallet-sdk-"));

    (client, ws, origin)
}

/// Answers a full node request with the network info of testnet11.
async fn answer_network_info(ws: &mut ServerSocket, origin: &str) {
    let request = recv_json(ws).await;
    assert_eq!(request["command"], "get_network_info");
    assert_eq!(request["destination"], "chia_full_node");
    assert_eq!(request["origin"], origin);
    assert_eq!(request["ack"], false);

    send_json(
        ws,
        &json!({
            "command": "get_network_info",
            "ack": true,
            "origin": "chia_full_node",
            "destination": origin,
            "request_id": request["request_id"],
            "data": { "success": true, "network_name": "testnet11", "network_prefix": "txch" },
        }),
    )
    .await;
}

async fn assert_network_info(client: &DaemonClient, ws: &mut ServerSocket, origin: &str) {
    let (response, ()) =
        within(async { tokio::join!(client.get_network_info(), answer_network_info(ws, origin)) })
            .await;
    let response = response.unwrap();
    assert!(response.success);
    assert_eq!(response.network_name.as_deref(), Some("testnet11"));
    assert_eq!(response.network_prefix.as_deref(), Some("txch"));
}

#[tokio::test]
async fn forwards_requests_and_events_then_closes() -> Result<()> {
    let daemon = FakeDaemon::start().await?;
    let (client, mut ws, origin) = accept_client(&daemon).await;

    // Subscribing registers the service once, even if it's subscribed again.
    let mut events = client.subscribe("wallet_ui").await?;
    assert_eq!(
        within(expect_registration(&mut ws, Some(&origin))).await,
        "wallet_ui"
    );
    let mut second = client.subscribe("wallet_ui").await?;

    assert_network_info(&client, &mut ws, &origin).await;

    // A message that doesn't match a pending request is broadcast to every subscription.
    send_json(
        &mut ws,
        &json!({
            "command": "state_changed",
            "ack": false,
            "origin": "chia_wallet",
            "destination": "wallet_ui",
            "request_id": "not-a-request",
            "data": { "state": "sync_changed" },
        }),
    )
    .await;

    for receiver in [&mut events, &mut second] {
        let event = within(receiver.recv()).await?;
        assert_eq!(event.command, "state_changed");
        assert_eq!(event.origin, "chia_wallet");
        assert_eq!(event.data, json!({ "state": "sync_changed" }));
    }

    // Closing sends a close frame, and the server's reply stops the client for good.
    client.close().await?;
    within(async {
        while let Some(message) = ws.next().await {
            if message.is_err() {
                break;
            }
        }
    })
    .await;
    within(client.closed()).await;

    assert!(matches!(
        client.get_network_info().await,
        Err(DaemonError::ConnectionClosed)
    ));

    Ok(())
}

#[tokio::test]
async fn reconnects_and_registers_services_again() -> Result<()> {
    let daemon = FakeDaemon::start().await?;
    let (client, mut ws, origin) = accept_client(&daemon).await;

    let mut disconnects = client.on_disconnect();
    let mut reconnects = client.on_reconnect();

    let _events = client.subscribe("wallet_ui").await?;
    assert_eq!(
        within(expect_registration(&mut ws, Some(&origin))).await,
        "wallet_ui"
    );

    // Dropping the socket without a close handshake makes the client reconnect.
    drop(ws);
    let mut ws = within(daemon.accept()).await;
    within(disconnects.recv()).await?;

    // The client registers its origin first, then each subscribed service.
    assert_eq!(
        within(expect_registration(&mut ws, Some(&origin))).await,
        origin
    );
    assert_eq!(
        within(expect_registration(&mut ws, Some(&origin))).await,
        "wallet_ui"
    );
    within(reconnects.recv()).await?;

    assert_network_info(&client, &mut ws, &origin).await;

    Ok(())
}

#[tokio::test]
async fn stops_after_max_reconnect_attempts() -> Result<()> {
    let daemon = FakeDaemon::start().await?;
    let (client, ws, _origin) = accept_client(&daemon).await;
    client.set_max_reconnect_attempts(Some(1));

    // Nothing is listening anymore, so the only reconnection attempt fails.
    drop(daemon);
    drop(ws);
    within(client.closed()).await;

    assert!(matches!(
        client.get_network_info().await,
        Err(DaemonError::ConnectionClosed)
    ));

    Ok(())
}

/// Reads from the socket until the client closes it or the connection ends.
async fn drain(ws: &mut ServerSocket) {
    while let Some(message) = ws.next().await {
        if message.is_err() {
            break;
        }
    }
}

#[tokio::test]
async fn close_during_reconnect_stops_the_client() -> Result<()> {
    let daemon = FakeDaemon::start().await?;
    let addr = daemon.addr();
    let (client, ws, _origin) = accept_client(&daemon).await;
    let mut disconnects = client.on_disconnect();
    let mut reconnects = client.on_reconnect();

    // Nothing is listening, so the client keeps waiting between reconnection attempts.
    drop(daemon);
    drop(ws);
    within(disconnects.recv()).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    within(client.close()).await?;

    // The daemon comes back, but the client stays closed instead of reconnecting to it.
    let daemon = FakeDaemon::bind(addr).await?;
    within(async {
        tokio::select! {
            () = client.closed() => {}
            _ = daemon.accept() => panic!("the client reconnected after it was closed"),
        }
    })
    .await;

    let accepted = tokio::time::timeout(Duration::from_secs(3), daemon.listener.accept()).await;
    assert!(
        accepted.is_err(),
        "the client reconnected after it was closed"
    );
    assert!(reconnects.try_recv().is_err());

    assert!(matches!(
        client.get_network_info().await,
        Err(DaemonError::ConnectionClosed)
    ));
    assert!(matches!(
        client.subscribe("wallet_ui").await,
        Err(DaemonError::ConnectionClosed)
    ));

    Ok(())
}

#[tokio::test]
async fn pending_request_fails_when_the_daemon_closes() -> Result<()> {
    let daemon = FakeDaemon::start().await?;
    let (client, mut ws, _origin) = accept_client(&daemon).await;

    // The daemon closes the connection instead of answering the request.
    let server = async {
        let request = recv_json(&mut ws).await;
        assert_eq!(request["command"], "get_network_info");
        ws.close(None).await.unwrap();
        drain(&mut ws).await;
    };

    // The request fails as soon as the connection closes, well before its timeout.
    let (response, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(client.get_network_info(), server)
    })
    .await
    .expect("the request didn't fail promptly");
    assert!(matches!(response, Err(DaemonError::ReceiveFailed)));

    Ok(())
}

#[tokio::test]
async fn closed_fires_when_the_daemon_closes() -> Result<()> {
    let daemon = FakeDaemon::start().await?;
    let (client, mut ws, _origin) = accept_client(&daemon).await;
    let mut reconnects = client.on_reconnect();

    ws.close(None).await?;
    within(async { tokio::join!(client.closed(), drain(&mut ws)) }).await;

    assert!(reconnects.try_recv().is_err());
    assert!(matches!(
        client.get_network_info().await,
        Err(DaemonError::ConnectionClosed)
    ));

    Ok(())
}

#[tokio::test]
async fn subscribe_after_close_returns_connection_closed() -> Result<()> {
    let daemon = FakeDaemon::start().await?;
    let (client, mut ws, _origin) = accept_client(&daemon).await;

    // Subscribing fails as soon as the client starts closing, and after it has closed.
    client.close().await?;
    assert!(matches!(
        client.subscribe("wallet_ui").await,
        Err(DaemonError::ConnectionClosed)
    ));

    within(async { tokio::join!(client.closed(), drain(&mut ws)) }).await;
    assert!(matches!(
        client.subscribe("wallet_ui").await,
        Err(DaemonError::ConnectionClosed)
    ));

    Ok(())
}
