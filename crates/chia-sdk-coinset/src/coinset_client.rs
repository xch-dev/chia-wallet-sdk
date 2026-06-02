use reqwest::Client;
use serde::{Serialize, de::DeserializeOwned};

use crate::{ChiaRpcClient, ClientOptions};

#[derive(Debug, Clone)]
pub struct CoinsetClient {
    base_url: String,
    client: Client,
}

const TESTNET11_URL: &str = "https://testnet11.api.coinset.org";
const MAINNET_URL: &str = "https://api.coinset.org";

impl CoinsetClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
        }
    }

    /// Creates a client with opt-in [`ClientOptions`] (e.g. request timeouts).
    pub fn with_options(base_url: String, options: ClientOptions) -> reqwest::Result<Self> {
        Ok(Self {
            base_url,
            client: options.apply(Client::builder()).build()?,
        })
    }

    pub fn testnet11() -> Self {
        Self::new(TESTNET11_URL.to_string())
    }

    /// Creates a testnet11 client with opt-in [`ClientOptions`] (e.g. request timeouts).
    pub fn testnet11_with_options(options: ClientOptions) -> reqwest::Result<Self> {
        Self::with_options(TESTNET11_URL.to_string(), options)
    }

    pub fn mainnet() -> Self {
        Self::new(MAINNET_URL.to_string())
    }

    /// Creates a mainnet client with opt-in [`ClientOptions`] (e.g. request timeouts).
    pub fn mainnet_with_options(options: ClientOptions) -> reqwest::Result<Self> {
        Self::with_options(MAINNET_URL.to_string(), options)
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
