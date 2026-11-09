//! An HTTP client for the Chia full node RPC API, as served by [coinset.org](https://coinset.org)
//! or by a full node.
//!
//! The [`ChiaRpcClient`] trait has a method for each supported RPC endpoint, such as
//! [`get_coin_records_by_puzzle_hash`](ChiaRpcClient::get_coin_records_by_puzzle_hash) and
//! [`push_tx`](ChiaRpcClient::push_tx). It's implemented by:
//!
//! * [`CoinsetClient`], for the public coinset.org API on mainnet or testnet11.
//! * `FullNodeClient`, for the RPC server of your own full node, which authenticates with the
//!   node's private certificate. This requires the `native-tls` or `rustls` feature, and isn't
//!   available on `wasm32`.
//! * [`MockRpcClient`], which returns canned responses for tests.
//!
//! The `chia-sdk-daemon` crate also implements it for a connection to the Chia daemon.
//!
//! Responses are plain data types like [`GetCoinRecordsResponse`] and [`CoinRecord`]. Errors
//! reported by the server are returned in each response's `success` and `error` fields rather
//! than as an [`Err`].
//!
//! # Features
//!
//! * `native-tls`: Uses the platform's TLS library for HTTPS requests, and enables `FullNodeClient`.
//! * `rustls`: Uses `rustls` for HTTPS requests, and enables `FullNodeClient`.
//!
//! No TLS backend is enabled by default, so outside of `wasm32` one of these features is
//! needed to send requests to an `https://` URL such as coinset.org.
//!
//! # Example
//!
//! This example is `no_run` because it connects to the live coinset.org API.
//!
//! ```no_run
//! use chia_protocol::Bytes32;
//! use chia_sdk_coinset::{ChiaRpcClient, CoinsetClient};
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = CoinsetClient::testnet11();
//! let puzzle_hash = Bytes32::new([0; 32]);
//!
//! let response = client
//!     .get_coin_records_by_puzzle_hash(puzzle_hash, None, None, Some(false), None)
//!     .await?;
//!
//! if !response.success {
//!     return Err(response.error.unwrap_or_default().into());
//! }
//!
//! for record in response.coin_records.unwrap_or_default() {
//!     println!("{} has {} mojos", record.coin.coin_id(), record.coin.amount);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! See [`MockRpcClient`] for an example that runs without a network connection.

#![cfg_attr(not(test), warn(missing_docs))]

mod chia_rpc_client;
mod coinset_client;
mod mock_client;
mod models;
mod types;

pub use chia_rpc_client::*;
pub use coinset_client::*;
pub use mock_client::*;
pub use models::*;
pub use types::*;

#[cfg(all(
    any(feature = "native-tls", feature = "rustls"),
    not(target_arch = "wasm32")
))]
mod full_node_client;

#[cfg(all(
    any(feature = "native-tls", feature = "rustls"),
    not(target_arch = "wasm32")
))]
pub use full_node_client::*;
