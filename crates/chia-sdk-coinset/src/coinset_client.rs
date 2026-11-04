use std::time::Duration;

use reqwest::Client;
use serde::{Serialize, de::DeserializeOwned};

use crate::ChiaRpcClient;

/// The default amount of time to wait for an RPC request to complete.
pub const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct CoinsetClient {
    base_url: String,
    client: Client,
    timeout: Duration,
}

impl CoinsetClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
            timeout: DEFAULT_RPC_TIMEOUT,
        }
    }

    pub fn testnet11() -> Self {
        Self::new("https://testnet11.api.coinset.org".to_string())
    }

    pub fn mainnet() -> Self {
        Self::new("https://api.coinset.org".to_string())
    }

    /// Sets how long to wait for each request to complete.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl ChiaRpcClient for CoinsetClient {
    type Error = reqwest::Error;

    fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn make_post_request<R, B>(&self, endpoint: &str, body: B) -> Result<R, Self::Error>
    where
        B: Serialize + Send,
        R: DeserializeOwned + Send,
    {
        let url = format!("{}/{}", self.base_url(), endpoint);
        post_json(&self.client, &url, self.timeout, &body).await
    }
}

/// Posts a JSON body and parses the JSON response.
///
/// If the server responds with an HTTP error status, the body is still returned if it can be
/// parsed, since RPC errors are often reported that way. Otherwise, the status error is returned.
pub(crate) async fn post_json<R, B>(
    client: &Client,
    url: &str,
    timeout: Duration,
    body: &B,
) -> reqwest::Result<R>
where
    B: Serialize,
    R: DeserializeOwned,
{
    let res = client.post(url).timeout(timeout).json(body).send().await?;

    if let Err(error) = res.error_for_status_ref().map(|_| ()) {
        let bytes = res.bytes().await?;
        return serde_json::from_slice(&bytes).map_err(|_| error);
    }

    res.json::<R>().await
}
