use serde::{Deserialize, Serialize};

/// A JSON message sent to the Chia daemon over its websocket.
///
/// # Example
///
/// ```
/// use chia_sdk_daemon::WebsocketRequest;
///
/// let request = WebsocketRequest {
///     command: "get_blockchain_state".to_string(),
///     ack: false,
///     origin: "my-wallet".to_string(),
///     destination: "chia_full_node".to_string(),
///     request_id: "1".to_string(),
///     data: serde_json::json!({}),
/// };
///
/// let json = serde_json::to_value(&request)?;
/// assert_eq!(json["destination"], "chia_full_node");
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct WebsocketRequest {
    /// The command to run, such as `register_service` or an RPC endpoint name.
    pub command: String,
    /// Whether this message is a response. Requests set this to `false`.
    pub ack: bool,
    /// The service name of the sender, which the daemon uses to route responses back.
    pub origin: String,
    /// The service that handles the command, such as `daemon` or `chia_full_node`.
    pub destination: String,
    /// An id that's echoed back in the response, so it can be matched to the request.
    pub request_id: String,
    /// The parameters of the command.
    pub data: serde_json::Value,
}

/// A JSON message received from the Chia daemon over its websocket.
///
/// This is either a response to a [`WebsocketRequest`] with the same `request_id`, or an
/// event that the daemon forwarded from a service.
#[derive(Debug, Clone, Deserialize)]
pub struct WebsocketResponse {
    /// The command this message responds to, or the name of the event.
    pub command: String,
    /// Whether this message is a response to a request.
    pub ack: bool,
    /// The service that sent the message.
    pub origin: String,
    /// The service the message is addressed to.
    pub destination: String,
    /// The id of the request this message responds to.
    pub request_id: String,
    /// The result of the command, or the event's data.
    pub data: serde_json::Value,
}

/// A message from the daemon that wasn't a response to one of this client's pending requests,
/// such as a state change or metrics event from a service.
///
/// These are received from the receiver returned by `DaemonClient::subscribe`.
#[derive(Debug, Clone)]
pub struct DaemonEvent {
    /// The command or event name.
    pub command: String,
    /// The service that sent the event.
    pub origin: String,
    /// The event's data.
    pub data: serde_json::Value,
}
