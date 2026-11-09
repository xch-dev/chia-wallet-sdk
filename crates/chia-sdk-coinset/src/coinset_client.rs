use reqwest::Client;
use serde::{Serialize, de::DeserializeOwned};

use crate::ChiaRpcClient;

/// A [`ChiaRpcClient`] for the [coinset.org](https://coinset.org) API, or any other server
/// that accepts full node RPC requests without client authentication.
///
/// Outside of `wasm32`, the `native-tls` or `rustls` feature must be enabled to connect to
/// an `https://` URL such as coinset.org.
///
/// # Example
///
/// This example is `no_run` because it connects to the live coinset.org API.
///
/// ```no_run
/// use chia_sdk_coinset::{ChiaRpcClient, CoinsetClient};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), reqwest::Error> {
/// let client = CoinsetClient::mainnet();
/// let response = client.get_blockchain_state().await?;
///
/// if let Some(state) = response.blockchain_state {
///     println!("Peak height: {}", state.peak.height);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct CoinsetClient {
    base_url: String,
    client: Client,
}

impl CoinsetClient {
    /// Creates a client that sends requests to `{base_url}/{endpoint}`.
    ///
    /// The `base_url` shouldn't have a trailing slash.
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
        }
    }

    /// Creates a client for the coinset.org testnet11 API, `https://testnet11.api.coinset.org`.
    pub fn testnet11() -> Self {
        Self::new("https://testnet11.api.coinset.org".to_string())
    }

    /// Creates a client for the coinset.org mainnet API, `https://api.coinset.org`.
    pub fn mainnet() -> Self {
        Self::new("https://api.coinset.org".to_string())
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
        let res = self.client.post(&url).json(&body).send().await?;
        res.json::<R>().await
    }
}
