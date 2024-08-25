use std::io;

use chia_protocol::ProtocolMessageTypes;
use chia_sdk_client::ClientError;
use chia_sdk_signer::SignerError;
use futures_channel::mpsc::SendError;
use thiserror::Error;
use tokio_tungstenite::tungstenite;

use crate::SimulatorError;

#[derive(Debug, Error)]
pub enum PeerSimulatorError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("websocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),

    #[error("client error: {0}")]
    Client(#[from] ClientError),

    #[error("message parser error: {0}")]
    Streamable(#[from] chia_traits::Error),

    #[error("consensus error: {0}")]
    Consensus(#[from] chia_consensus::error::Error),

    #[error("signer error: {0}")]
    Signer(#[from] SignerError),

    #[error("simulator error: {0}")]
    Simulator(#[from] SimulatorError),

    #[error("send message error: {0}")]
    SendMessage(#[from] SendError),

    #[error("unsupported protocol message type: {0:?}")]
    UnsupportedMessage(ProtocolMessageTypes),
}
