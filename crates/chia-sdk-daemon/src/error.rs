use std::time::Duration;

/// An error that occurs while communicating with the Chia daemon.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    /// The websocket connection failed or couldn't be closed.
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),

    /// A request couldn't be serialized, or a response couldn't be parsed as the expected type.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The daemon didn't respond within the timeout passed to `DaemonClient::connect`.
    #[error("Request timed out after {0:?}")]
    Timeout(Duration),

    /// The connection was lost and hasn't been re-established yet.
    #[error("Connection closed")]
    ConnectionClosed,

    /// The message couldn't be sent over the websocket.
    #[error("Failed to send message")]
    SendFailed,

    /// The connection was lost while waiting for a response.
    #[error("Failed to receive response")]
    ReceiveFailed,

    /// Reconnecting gave up after this many attempts. The background task logs this error
    /// rather than returning it to a caller.
    #[error("Reconnection failed after {0} attempts")]
    ReconnectFailed(u32),
}
