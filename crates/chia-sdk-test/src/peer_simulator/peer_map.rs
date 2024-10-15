use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use futures_channel::mpsc::UnboundedSender;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

pub(crate) type Ws = UnboundedSender<Message>;
type Peers = HashMap<SocketAddr, Ws>;

#[derive(Default, Clone)]
pub(crate) struct PeerMap(Arc<Mutex<Peers>>);

impl PeerMap {
    pub(crate) async fn insert(&self, addr: SocketAddr, ws: Ws) {
        self.0.lock().await.insert(addr, ws);
    }

    pub(crate) async fn remove(&self, addr: SocketAddr) {
        self.0.lock().await.remove(&addr);
    }

    pub(crate) async fn peers(&self) -> Vec<(SocketAddr, Ws)> {
        self.0
            .lock()
            .await
            .iter()
            .map(|(addr, ws)| (*addr, ws.clone()))
            .collect()
    }
}
