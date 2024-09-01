use chia_protocol::{NodeType, ProtocolMessageTypes};
use thiserror::Error;
use tokio::sync::oneshot::error::RecvError;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("SSL error: {0}")]
    Ssl(#[from] chia_ssl::Error),

    #[error("TLS method is not supported")]
    UnsupportedTls,

    #[error("Streamable error: {0}")]
    Streamable(#[from] chia_traits::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),

    #[cfg(feature = "native-tls")]
    #[error("Native TLS error: {0}")]
    NativeTls(#[from] native_tls::Error),

    #[cfg(feature = "rustls")]
    #[error("Rustls error: {0}")]
    Rustls(#[from] rustls::Error),

    #[cfg(feature = "rustls")]
    #[error("Missing pkcs8 private key")]
    MissingPkcs8Key,

    #[cfg(feature = "rustls")]
    #[error("Missing CA cert")]
    MissingCa,

    #[error("Unexpected message received with type {0:?}")]
    UnexpectedMessage(ProtocolMessageTypes),

    #[error("Expected response with type {0:?}, found {1:?}")]
    InvalidResponse(Vec<ProtocolMessageTypes>, ProtocolMessageTypes),

    #[error("Failed to receive message")]
    Recv(#[from] RecvError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Missing response during handshake")]
    MissingHandshake,

    #[error("Expected node type {0:?}, but found {1:?}")]
    WrongNodeType(NodeType, NodeType),

    #[error("Expected network {0}, but found {1}")]
    WrongNetwork(String, String),

    #[error("The peer is banned")]
    BannedPeer,
}
