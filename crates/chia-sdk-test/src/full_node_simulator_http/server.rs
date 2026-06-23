use std::{
    net::SocketAddr,
    ops::Deref,
    sync::{Arc, Mutex},
};

use tokio::{net::TcpListener, task::JoinHandle};

use crate::FullNodeSimulator;

use super::{handlers::router, state::SharedSimulator};

#[derive(Debug)]
pub struct FullNodeSimulatorServer {
    addr: SocketAddr,
    simulator: SharedSimulator,
    join_handle: JoinHandle<()>,
}

impl Deref for FullNodeSimulatorServer {
    type Target = Mutex<FullNodeSimulator>;

    fn deref(&self) -> &Self::Target {
        &self.simulator
    }
}

impl FullNodeSimulatorServer {
    pub async fn new() -> std::io::Result<Self> {
        Self::with_simulator(Arc::new(Mutex::new(FullNodeSimulator::default()))).await
    }

    pub async fn with_simulator(simulator: SharedSimulator) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let app = router(simulator.clone());
        let join_handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Ok(Self {
            addr,
            simulator,
            join_handle,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for FullNodeSimulatorServer {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}
