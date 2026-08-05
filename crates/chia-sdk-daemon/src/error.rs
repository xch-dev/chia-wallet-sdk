use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Request timed out after {0:?}")]
    Timeout(Duration),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Failed to send message")]
    SendFailed,

    #[error("Failed to receive response")]
    ReceiveFailed,

    #[error("Reconnection failed after {0} attempts")]
    ReconnectFailed(u32),
}
