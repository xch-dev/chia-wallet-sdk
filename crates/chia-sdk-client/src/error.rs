use chia_protocol::{NodeType, ProtocolMessageTypes};
use thiserror::Error;
use tokio::sync::oneshot::error::RecvError;

/// An error that occurs while connecting to or communicating with a peer.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Generating a TLS certificate failed.
    #[error("SSL error: {0}")]
    Ssl(#[from] chia_ssl::Error),

    /// The websocket uses a kind of stream that isn't supported by the enabled TLS features.
    #[error("TLS method is not supported")]
    UnsupportedTls,

    /// A message couldn't be serialized or deserialized.
    #[error("Streamable error: {0}")]
    Streamable(#[from] chia_traits::Error),

    /// The websocket connection failed, or a message couldn't be sent over it.
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),

    /// Building the `native-tls` connector failed.
    #[cfg(feature = "native-tls")]
    #[error("Native TLS error: {0}")]
    NativeTls(#[from] native_tls::Error),

    /// Building the `rustls` connector failed.
    #[cfg(feature = "rustls")]
    #[error("Rustls error: {0}")]
    Rustls(#[from] rustls::Error),

    /// A certificate or key couldn't be parsed from PEM.
    #[cfg(feature = "rustls")]
    #[error("PEM error: {0}")]
    Pem(#[from] rustls::pki_types::pem::Error),

    /// The key PEM didn't contain a PKCS #8 private key.
    #[cfg(feature = "rustls")]
    #[error("Missing pkcs8 private key")]
    MissingPkcs8Key,

    /// The Chia CA certificate PEM didn't contain a certificate.
    #[cfg(feature = "rustls")]
    #[error("Missing CA cert")]
    MissingCa,

    /// The peer sent a response with a request id that wasn't waiting for a response.
    #[error("Unexpected message received with type {0:?}")]
    UnexpectedMessage(ProtocolMessageTypes),

    /// The peer responded with a different message type than expected. The fields are the
    /// expected message types, followed by the one that was received.
    #[error("Expected response with type {0:?}, found {1:?}")]
    InvalidResponse(Vec<ProtocolMessageTypes>, ProtocolMessageTypes),

    /// The channel for a response was closed before the response was received.
    #[error("Failed to receive message")]
    Recv(#[from] RecvError),

    /// An I/O error, such as failing to read the peer's address or a certificate file, or
    /// failing to resolve a DNS introducer.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// The connection closed before the peer responded to the handshake.
    #[error("Missing response during handshake")]
    MissingHandshake,

    /// The peer isn't the expected kind of node. The fields are the expected node type,
    /// followed by the one the peer reported.
    #[error("Expected node type {0:?}, but found {1:?}")]
    WrongNodeType(NodeType, NodeType),

    /// The peer is on a different network. The fields are the expected network id, followed
    /// by the one the peer reported.
    #[error("Expected network {0}, but found {1}")]
    WrongNetwork(String, String),

    /// The peer's IP address has been banned with `ClientState::ban`.
    #[error("The peer is banned")]
    BannedPeer,
}
