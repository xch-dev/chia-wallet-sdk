use std::io;

use chia_protocol::ProtocolMessageTypes;
use chia_sdk_client::ClientError;
use chia_sdk_signer::SignerError;
use futures_channel::mpsc::SendError;
use thiserror::Error;
use tokio_tungstenite::tungstenite;

use crate::SimulatorError;

/// An error returned by the [`PeerSimulator`](crate::PeerSimulator).
#[derive(Debug, Error)]
pub enum PeerSimulatorError {
    /// An I/O error, such as failing to bind the listener.
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    /// A websocket error.
    #[error("websocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),

    /// An error from the `chia-sdk-client` peer.
    #[error("client error: {0}")]
    Client(#[from] ClientError),

    /// A protocol message couldn't be parsed or serialized.
    #[error("message parser error: {0}")]
    Streamable(#[from] chia_traits::Error),

    /// A consensus error.
    #[error("consensus error: {0}")]
    Consensus(#[from] chia_consensus::error::Error),

    /// The required signatures couldn't be calculated.
    #[error("signer error: {0}")]
    Signer(#[from] SignerError),

    /// An error from the underlying [`Simulator`](crate::Simulator).
    #[error("simulator error: {0}")]
    Simulator(#[from] SimulatorError),

    /// A message couldn't be sent to a connected peer.
    #[error("send message error: {0}")]
    SendMessage(#[from] SendError),

    /// A peer sent a request type that the simulator doesn't support.
    #[error("unsupported protocol message type: {0:?}")]
    UnsupportedMessage(ProtocolMessageTypes),
}
