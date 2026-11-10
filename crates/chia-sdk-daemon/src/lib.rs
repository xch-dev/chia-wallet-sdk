//! A websocket client for the Chia daemon.
//!
//! The daemon is the process that starts and manages the other Chia services, such as the
//! full node and wallet, on a machine. It accepts websocket connections on port 55400 by
//! default, and routes JSON messages between clients and services.
//!
//! `DaemonClient` connects to the daemon and implements
//! [`ChiaRpcClient`](chia_sdk_coinset::ChiaRpcClient) by forwarding requests to the full node,
//! so the same code can query a node through coinset.org, its RPC server, or the daemon.
//! It can also subscribe to events that services broadcast, and reconnects automatically if
//! the connection is lost.
//!
//! # Features
//!
//! `DaemonClient` requires one of these features, which enable the matching TLS backend in
//! `chia-sdk-client`:
//!
//! * `native-tls`: Uses the platform's TLS library.
//! * `rustls`: Uses `rustls`.
//!
//! Without either, only the message types such as [`WebsocketRequest`] and [`DaemonError`]
//! are available.
//!
//! # Example
//!
//! This example is `no_run` because it connects to a Chia daemon running locally.
//!
//! ```no_run
//! # #[cfg(feature = "rustls")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use std::time::Duration;
//!
//! use chia_sdk_client::{create_rustls_connector, load_ssl_cert};
//! use chia_sdk_coinset::ChiaRpcClient;
//! use chia_sdk_daemon::DaemonClient;
//!
//! let cert = load_ssl_cert("private_daemon.crt", "private_daemon.key")?;
//! let connector = create_rustls_connector(&cert)?;
//! let client =
//!     DaemonClient::connect("wss://localhost:55400", connector, Duration::from_secs(30)).await?;
//!
//! let response = client.get_network_info().await?;
//! println!("The full node is on {:?}", response.network_name);
//!
//! client.close().await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(not(test), warn(missing_docs))]

mod client;
mod error;
mod types;

#[cfg(any(feature = "native-tls", feature = "rustls"))]
pub use client::*;
pub use error::*;
pub use types::*;
