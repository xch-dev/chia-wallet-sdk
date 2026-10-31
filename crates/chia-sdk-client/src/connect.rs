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
    let connect = async {
        let (peer, receiver) = Peer::connect(socket_addr, connector, options).await?;
        handshake(peer, receiver, network_id).await
    };

    match options.handshake_timeout {
        Some(duration) => tokio::time::timeout(duration, connect)
            .await
            .map_err(|_| ClientError::HandshakeTimeout(duration))?,
        None => connect.await,
    }
}

async fn handshake(
    peer: Peer,
    mut receiver: mpsc::Receiver<Message>,
    network_id: String,
) -> Result<(Peer, mpsc::Receiver<Message>), ClientError> {
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
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chia_ssl::ChiaCertificate;
    use tokio::net::TcpListener;

    use super::*;

    #[tokio::test]
    async fn test_handshake_timeout() -> anyhow::Result<()> {
        // Accept the TCP connection, but never complete the TLS or websocket handshake.
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let socket_addr = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await?;
            tokio::time::sleep(Duration::from_secs(60)).await;
            drop(stream);
            anyhow::Ok(())
        });

        let cert = ChiaCertificate::generate()?;

        #[cfg(feature = "native-tls")]
        let connector = crate::create_native_tls_connector(&cert)?;

        #[cfg(not(feature = "native-tls"))]
        let connector = crate::create_rustls_connector(&cert)?;

        let timeout = Duration::from_millis(200);
        let options = PeerOptions {
            handshake_timeout: Some(timeout),
            ..Default::default()
        };

        let result = tokio::time::timeout(
            Duration::from_secs(10),
            connect_peer("mainnet".to_string(), connector, socket_addr, options),
        )
        .await?;

        assert!(
            matches!(result, Err(ClientError::HandshakeTimeout(duration)) if duration == timeout)
        );

        server.abort();

        Ok(())
    }
}
