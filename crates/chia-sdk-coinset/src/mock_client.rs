use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Mutex;

use crate::ChiaRpcClient;

/// A [`ChiaRpcClient`] that returns canned responses instead of making network requests,
/// for use in tests.
///
/// Responses are keyed by the full URL, which is `http://api.example.com/{endpoint}`. Every
/// request is recorded, so tests can check what was sent.
///
/// # Example
///
/// ```
/// use chia_sdk_coinset::{ChiaRpcClient, MockRpcClient};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut client = MockRpcClient::new();
/// client.mock_response(
///     "http://api.example.com/get_network_info",
///     r#"{"network_name": "mainnet", "network_prefix": "xch", "success": true}"#,
/// );
///
/// let response = client.get_network_info().await?;
/// assert_eq!(response.network_prefix.as_deref(), Some("xch"));
///
/// let requests = client.get_requests();
/// assert_eq!(requests[0].0, "http://api.example.com/get_network_info");
///
/// // Requests without a mock response fail.
/// assert!(client.get_blockchain_state().await.is_err());
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct MockRpcClient {
    requests: Mutex<Vec<(String, Value)>>,
    responses: HashMap<String, String>,
}

impl MockRpcClient {
    /// Creates a client with no mock responses.
    pub fn new() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            responses: HashMap::new(),
        }
    }

    /// Sets the JSON response returned for requests to `url`, replacing any previous one.
    pub fn mock_response(&mut self, url: &str, response: &str) {
        self.responses.insert(url.to_string(), response.to_string());
    }

    /// Returns the URL and JSON body of every request made so far, in order.
    ///
    /// Requests that had no mock response are included.
    pub fn get_requests(&self) -> Vec<(String, Value)> {
        self.requests.lock().unwrap().clone()
    }

    /// Records a request and returns the mock response for `url`, or an error if there isn't one.
    pub fn post(&self, url: &str, json: Value) -> Result<String, Box<dyn Error>> {
        self.requests.lock().unwrap().push((url.to_string(), json));

        match self.responses.get(url) {
            Some(response) => Ok(response.clone()),
            None => Err("No mock response configured for URL".into()),
        }
    }
}

impl Default for MockRpcClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ChiaRpcClient for MockRpcClient {
    type Error = Box<dyn Error>;

    fn base_url(&self) -> &'static str {
        "http://api.example.com"
    }

    async fn make_post_request<R, B>(&self, endpoint: &str, body: B) -> Result<R, Self::Error>
    where
        B: Serialize,
        R: DeserializeOwned,
    {
        let url = format!("{}/{}", self.base_url(), endpoint);
        let body = serde_json::to_value(body)?;
        let response = self.post(&url, body)?;
        Ok(serde_json::from_str::<R>(&response)?)
    }
}
