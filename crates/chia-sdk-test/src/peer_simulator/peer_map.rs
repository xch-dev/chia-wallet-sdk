use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use futures_channel::mpsc::UnboundedSender;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

pub(crate) type Ws = UnboundedSender<Message>;

#[derive(Debug, Clone)]
pub(crate) struct ConnectedPeer {
    pub(crate) ws: Ws,
    /// Whether the peer advertised the CHIP-26 mempool updates capability in its handshake.
    pub(crate) mempool_updates: bool,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct PeerMap(Arc<Mutex<HashMap<SocketAddr, ConnectedPeer>>>);

impl PeerMap {
    pub(crate) async fn insert(&self, addr: SocketAddr, ws: Ws) {
        self.0.lock().await.insert(
            addr,
            ConnectedPeer {
                ws,
                mempool_updates: false,
            },
        );
    }

    pub(crate) async fn remove(&self, addr: SocketAddr) {
        self.0.lock().await.remove(&addr);
    }

    pub(crate) async fn set_mempool_updates(&self, addr: SocketAddr, mempool_updates: bool) {
        if let Some(peer) = self.0.lock().await.get_mut(&addr) {
            peer.mempool_updates = mempool_updates;
        }
    }

    pub(crate) async fn mempool_updates(&self, addr: SocketAddr) -> bool {
        self.0
            .lock()
            .await
            .get(&addr)
            .is_some_and(|peer| peer.mempool_updates)
    }

    pub(crate) async fn peers(&self) -> Vec<(SocketAddr, ConnectedPeer)> {
        self.0
            .lock()
            .await
            .iter()
            .map(|(addr, peer)| (*addr, peer.clone()))
            .collect()
    }
}
