use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use tokio::{net::TcpListener, task::JoinHandle};

use crate::FullNodeSimulator;

use super::{handlers::router, state::SharedSimulator};

/// A local HTTP server that exposes a [`FullNodeSimulator`] through the full node RPC.
///
/// The server listens on an ephemeral port on `127.0.0.1` and runs on a Tokio task, so it must be
/// created inside a Tokio runtime. It stops when this value is dropped.
///
/// All endpoints accept a JSON body with `POST`. Along with the standard full node endpoints
/// (such as `get_blockchain_state`, `get_coin_records_by_puzzle_hash`, and `push_tx`), it serves
/// `get_aggsig_additional_data` and simulator controls under `/sim/`: `farm_block`,
/// `revert_blocks`, `reorg_blocks`, `new_coin`, `insert_coin`, `set_farming_ph`, `dump_state`,
/// `restore_state`, and `drain_events`.
///
/// # Example
///
/// ```
/// use chia_sdk_coinset::{ChiaRpcClient, CoinsetClient};
/// use chia_sdk_test::FullNodeSimulatorServer;
///
/// # tokio::runtime::Runtime::new()?.block_on(async {
/// let server = FullNodeSimulatorServer::new().await?;
/// let client = CoinsetClient::new(server.url());
///
/// let state = client.get_blockchain_state().await?;
/// assert_eq!(state.blockchain_state.unwrap().peak.height, 1);
/// # Ok::<(), anyhow::Error>(())
/// # })?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug)]
pub struct FullNodeSimulatorServer {
    addr: SocketAddr,
    join_handle: JoinHandle<()>,
}

impl FullNodeSimulatorServer {
    /// Starts a server for a new [`FullNodeSimulator`] with the default seed.
    ///
    /// # Errors
    ///
    /// Returns an error if the listener can't be bound.
    pub async fn new() -> std::io::Result<Self> {
        Self::with_simulator(Arc::new(Mutex::new(FullNodeSimulator::default()))).await
    }

    /// Starts a server for an existing simulator, shared as an
    /// `Arc<std::sync::Mutex<FullNodeSimulator>>`.
    ///
    /// Keep a clone of the [`Arc`] to inspect or control the simulator directly while the server
    /// is running.
    ///
    /// # Errors
    ///
    /// Returns an error if the listener can't be bound.
    pub async fn with_simulator(simulator: SharedSimulator) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let app = router(simulator.clone());
        let join_handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Ok(Self { addr, join_handle })
    }

    /// Returns the address the server is listening on.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Returns the base URL of the server, such as `http://127.0.0.1:12345`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for FullNodeSimulatorServer {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}
