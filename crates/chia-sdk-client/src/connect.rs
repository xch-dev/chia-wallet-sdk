use std::net::SocketAddr;

use chia_protocol::{Handshake, Message, NodeType, ProtocolMessageTypes};
use chia_traits::Streamable;
use tokio::sync::mpsc;
use tokio_tungstenite::Connector;
use tracing::instrument;

use crate::{Capability, ClientError, Peer, PeerOptions};

/// Connects to a full node and performs the wallet protocol handshake.
///
/// The `network_id` is the name of the network, such as `mainnet` or `testnet11`. The
/// handshake identifies this side as a wallet, and fails with an error unless the first
/// message from the peer is a [`Handshake`] from a full node on the same network.
///
/// The handshake advertises [`Capability::Base`], [`Capability::BlockHeaders`] and
/// [`Capability::RateLimitsV2`], and also [`Capability::MempoolUpdates`] if
/// [`PeerOptions::mempool_updates`] is enabled.
///
/// Returns the [`Peer`] and the receiver for messages that aren't responses to requests.
/// See [`Peer`] for an example.
#[instrument(skip(connector))]
pub async fn connect_peer(
    network_id: String,
    connector: Connector,
    socket_addr: SocketAddr,
    options: PeerOptions,
) -> Result<(Peer, mpsc::Receiver<Message>), ClientError> {
    let (peer, receiver) = Peer::connect(socket_addr, connector, options).await?;
    handshake(peer, receiver, network_id, options).await
}

async fn handshake(
    peer: Peer,
    mut receiver: mpsc::Receiver<Message>,
    network_id: String,
    options: PeerOptions,
) -> Result<(Peer, mpsc::Receiver<Message>), ClientError> {
    let mut capabilities = vec![
        Capability::Base,
        Capability::BlockHeaders,
        Capability::RateLimitsV2,
    ];

    if options.mempool_updates {
        capabilities.push(Capability::MempoolUpdates);
    }

    peer.send(Handshake {
        network_id: network_id.clone(),
        protocol_version: "0.0.37".to_string(),
        software_version: "0.0.0".to_string(),
        server_port: 0,
        node_type: NodeType::Wallet,
        capabilities: capabilities
            .into_iter()
            .map(|capability| (capability.into(), "1".to_string()))
            .collect(),
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
    use chia_protocol::Bytes;
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, connect_async, tungstenite};

    use super::*;

    /// Runs the handshake against a local websocket server that acts as a full node, and
    /// returns the capabilities the wallet advertised.
    async fn advertised_capabilities(options: PeerOptions) -> anyhow::Result<Vec<(u16, String)>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await?;
            let mut ws = accept_async(stream).await?;

            let Some(tungstenite::Message::Binary(data)) = ws.next().await.transpose()? else {
                anyhow::bail!("expected a binary handshake message");
            };
            let message = Message::from_bytes(&data)?;
            assert_eq!(message.msg_type, ProtocolMessageTypes::Handshake);
            let handshake = Handshake::from_bytes(&message.data)?;
            assert_eq!(handshake.node_type, NodeType::Wallet);

            let response = Handshake {
                network_id: "testnet11".to_string(),
                protocol_version: "0.0.37".to_string(),
                software_version: "0.0.0".to_string(),
                server_port: 58444,
                node_type: NodeType::FullNode,
                capabilities: vec![(Capability::MempoolUpdates.into(), "1".to_string())],
            };
            let message = Message {
                msg_type: ProtocolMessageTypes::Handshake,
                id: None,
                data: Bytes::new(response.to_bytes()?),
            };
            ws.send(tungstenite::Message::Binary(message.to_bytes()?.into()))
                .await?;

            anyhow::Ok(handshake.capabilities)
        });

        let (ws, _) = connect_async(format!("ws://{addr}")).await?;
        let (peer, receiver) = Peer::from_websocket(ws, options)?;
        handshake(peer, receiver, "testnet11".to_string(), options).await?;

        server.await?
    }

    fn base_capabilities() -> Vec<(u16, String)> {
        vec![
            (1, "1".to_string()),
            (2, "1".to_string()),
            (3, "1".to_string()),
        ]
    }

    #[tokio::test]
    async fn test_handshake_without_mempool_updates() -> anyhow::Result<()> {
        let capabilities = advertised_capabilities(PeerOptions::default()).await?;
        assert_eq!(capabilities, base_capabilities());
        Ok(())
    }

    #[tokio::test]
    async fn test_handshake_with_mempool_updates() -> anyhow::Result<()> {
        let capabilities = advertised_capabilities(PeerOptions {
            mempool_updates: true,
            ..PeerOptions::default()
        })
        .await?;

        let mut expected = base_capabilities();
        expected.push((5, "1".to_string()));
        assert_eq!(capabilities, expected);
        Ok(())
    }
}
