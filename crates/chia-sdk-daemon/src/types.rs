use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct WebsocketRequest {
    pub command: String,
    pub ack: bool,
    pub origin: String,
    pub destination: String,
    pub request_id: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebsocketResponse {
    pub command: String,
    pub ack: bool,
    pub origin: String,
    pub destination: String,
    pub request_id: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct DaemonEvent {
    pub command: String,
    pub origin: String,
    pub data: serde_json::Value,
}
