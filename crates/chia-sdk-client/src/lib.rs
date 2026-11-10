//! Connects to Chia full nodes using the wallet protocol.
//!
//! This is what light wallets use to sync coins and submit transactions without running a
//! full node themselves. The main types are:
//!
//! * [`Peer`], a websocket connection to a single full node, with a method for each wallet
//!   protocol request, such as [`Peer::request_puzzle_state`] and [`Peer::send_transaction`].
//!   Messages that aren't responses, like coin state updates, are sent to a receiver.
//! * `connect_peer`, which connects to a peer and performs the wallet protocol handshake.
//! * `Client`, which manages connections to several peers, and keeps track of banned and
//!   trusted IP addresses.
//! * [`Network`], which describes a network and finds peers with its DNS introducers.
//! * [`load_ssl_cert`], `create_native_tls_connector`, and `create_rustls_connector`, which
//!   set up the TLS client certificate that full nodes require.
//! * [`RateLimiter`] and [`V2_RATE_LIMITS`], which keep outgoing messages within the limits
//!   that full nodes enforce.
//!
//! # Features
//!
//! Connecting to a peer requires TLS, so most of this crate needs one of these features:
//!
//! * `native-tls`: Uses the platform's TLS library, and enables `create_native_tls_connector`.
//! * `rustls`: Uses `rustls`, and enables `create_rustls_connector`.
//!
//! Either feature enables `Peer::connect`, `connect_peer`, `Client`, and the `Connector`
//! re-export from `tokio-tungstenite`. Without them, only [`Peer::from_websocket`] can create a
//! [`Peer`].
//!
//! # Example
//!
//! This example is `no_run` because it connects to live mainnet peers.
//!
//! ```no_run
//! # #[cfg(feature = "rustls")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use std::time::Duration;
//!
//! use chia_sdk_client::{Network, PeerOptions, connect_peer, create_rustls_connector, load_ssl_cert};
//!
//! // Full nodes require a client certificate, which is generated the first time.
//! let cert = load_ssl_cert("wallet.crt", "wallet.key")?;
//! let connector = create_rustls_connector(&cert)?;
//!
//! // Find peers with the mainnet DNS introducers.
//! let network = Network::default_mainnet();
//! let addrs = network.lookup_all(Duration::from_secs(3), 2).await;
//! let addr = *addrs.first().ok_or("no peers found")?;
//!
//! let (peer, _receiver) = connect_peer(
//!     "mainnet".to_string(),
//!     connector,
//!     addr,
//!     PeerOptions::default(),
//! )
//! .await?;
//!
//! let peers = peer.request_peers().await?;
//! println!("{} knows about {} other peers", peer.socket_addr(), peers.peer_list.len());
//! # Ok(())
//! # }
//! ```

#![cfg_attr(not(test), warn(missing_docs))]

mod error;
mod network;
mod peer;
mod rate_limiter;
mod rate_limits;
mod request_map;
mod tls;

pub use error::*;
pub use network::*;
pub use peer::*;
pub use rate_limiter::*;
pub use rate_limits::*;
pub use tls::*;

#[cfg(any(feature = "native-tls", feature = "rustls"))]
mod client;
#[cfg(any(feature = "native-tls", feature = "rustls"))]
mod connect;

#[cfg(any(feature = "native-tls", feature = "rustls"))]
pub use client::*;
#[cfg(any(feature = "native-tls", feature = "rustls"))]
pub use connect::*;

/// The TLS configuration used to open websocket connections, re-exported from
/// `tokio-tungstenite`. Create one with `create_native_tls_connector` or
/// `create_rustls_connector`.
#[cfg(any(feature = "native-tls", feature = "rustls"))]
pub use tokio_tungstenite::Connector;
