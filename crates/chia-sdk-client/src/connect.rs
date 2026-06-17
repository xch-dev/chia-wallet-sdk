use std::net::SocketAddr;

use chia_protocol::{Handshake, Message, NodeType, ProtocolMessageTypes};
use chia_traits::Streamable;
use tokio::sync::mpsc;
use tokio_tungstenite::Connector;
use tracing::instrument;

use crate::{ClientError, Peer, PeerOptions};

#[instrument(skip(connector))]
pub async fn connect_peer(
    network_id: String,
    connector: Connector,
    socket_addr: SocketAddr,
    options: PeerOptions,
) -> Result<(Peer, mpsc::Receiver<Message>), ClientError> {
    let connect_timeout = options.connect_timeout;

    // `connect_timeout` is a single budget covering both the websocket connect and the
    // chia handshake exchange. The inner `Peer::connect` also honors it for direct
    // (non-handshake) callers; in this path it is bounded by the outer timeout below.
    let inner = async move {
        let (peer, mut receiver) = Peer::connect(socket_addr, connector, options).await?;

        peer.send(Handshake {
            network_id: network_id.clone(),
            protocol_version: "0.0.37".to_string(),
            software_version: "0.0.0".to_string(),
            server_port: 0,
            node_type: NodeType::Wallet,
            capabilities: vec![
                (1, "1".to_string()),
                (2, "1".to_string()),
                (3, "1".to_string()),
            ],
        })
        .await?;

        let Some(message) = receiver.recv().await else {
            return Err(ClientError::MissingHandshake);
        };

        if message.msg_type != ProtocolMessageTypes::Handshake {
            return Err(ClientError::InvalidResponse(
                vec![ProtocolMessageTypes::Handshake],
                message.msg_type,
            ));
        }

        let handshake = Handshake::from_bytes(&message.data)?;

        if handshake.node_type != NodeType::FullNode {
            return Err(ClientError::WrongNodeType(
                NodeType::FullNode,
                handshake.node_type,
            ));
        }

        if handshake.network_id != network_id {
            return Err(ClientError::WrongNetwork(
                network_id.clone(),
                handshake.network_id,
            ));
        }

        Ok((peer, receiver))
    };

    match connect_timeout {
        Some(duration) => tokio::time::timeout(duration, inner)
            .await
            .map_err(|_| ClientError::Timeout(duration))?,
        None => inner.await,
    }
}
