use std::io;

use chia_consensus::gen::validation_error::ValidationErr;
use chia_protocol::ProtocolMessageTypes;
use chia_sdk_signer::ConditionError;
use futures_channel::mpsc::SendError;
use thiserror::Error;
use tokio_tungstenite::tungstenite;

#[derive(Debug, Error)]
pub enum SimulatorError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("websocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),

    #[error("message parser error: {0}")]
    Streamable(#[from] chia_traits::Error),

    #[error("consensus error: {0}")]
    Consensus(#[from] chia_consensus::error::Error),

    #[error("condition error: {0}")]
    Condition(#[from] ConditionError),

    #[error("validation error: {0}")]
    Validation(#[from] ValidationErr),

    #[error("send message error: {0}")]
    SendMessage(#[from] SendError),

    #[error("unsupported protocol message type: {0:?}")]
    UnsupportedMessage(ProtocolMessageTypes),
}
