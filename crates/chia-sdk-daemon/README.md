# chia-sdk-daemon

A Chia daemon websocket client. Connects to the Chia daemon over WSS (mTLS), supports RPC calls routed through the daemon (via the `ChiaRpcClient` trait), and event subscriptions via broadcast channels.

## Setup

Add the crate with one of the TLS features enabled:

```toml
[dependencies]
chia-sdk-daemon = { version = "0.33.0", features = ["native-tls"] }
# or
chia-sdk-daemon = { version = "0.33.0", features = ["rustls"] }
```

## Connecting to the Daemon

```rust
use std::time::Duration;

use chia_sdk_client::{create_native_tls_connector, load_ssl_cert};
use chia_sdk_daemon::DaemonClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cert = load_ssl_cert(
        "~/.chia/mainnet/config/ssl/daemon/private_daemon.crt",
        "~/.chia/mainnet/config/ssl/daemon/private_daemon.key",
    )?;
    let connector = create_native_tls_connector(&cert)?;

    let client = DaemonClient::connect(
        "wss://localhost:55400",
        connector,
        Duration::from_secs(30),
    )
    .await?;

    // ... use client ...

    client.close().await?;
    Ok(())
}
```

## Making RPC Calls

`DaemonClient` implements `ChiaRpcClient`, so all full_node RPC methods are available directly. Requests are routed through the daemon websocket to `chia_full_node`.

```rust
use chia_sdk_coinset::ChiaRpcClient;

let state = client.get_blockchain_state().await?;
println!("Peak height: {}", state.blockchain_state.unwrap().peak.height);

let record = client.get_block_record_by_height(1000).await?;
println!("Block record: {:?}", record.block_record);
```

## Subscribing to Events

Use `subscribe` to register for daemon events and receive them via a broadcast channel. This sends a `register_service` command to the daemon for the given service name.

```rust
use chia_sdk_daemon::DaemonEvent;

let mut receiver = client.subscribe("metrics").await?;

tokio::spawn(async move {
    loop {
        match receiver.recv().await {
            Ok(event) => {
                println!("Event: {} from {}", event.command, event.origin);
                println!("Data: {}", event.data);
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                eprintln!("Missed {n} events");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }
});
```
