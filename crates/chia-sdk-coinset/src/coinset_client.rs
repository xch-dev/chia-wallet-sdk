use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};

use crate::ChiaRpcClient;

#[derive(Debug)]
pub struct CoinsetClient {
    base_url: String,
    client: Client,
}

impl CoinsetClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
        }
    }

    pub fn testnet11() -> Self {
        Self::new("https://testnet11.api.coinset.org".to_string())
    }

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
