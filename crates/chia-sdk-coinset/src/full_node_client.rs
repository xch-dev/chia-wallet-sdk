use reqwest::{Client, Identity};
use serde::{Serialize, de::DeserializeOwned};

use crate::{ChiaRpcClient, ClientOptions};

#[derive(Debug)]
pub struct FullNodeClient {
    base_url: String,
    client: Client,
}

impl FullNodeClient {
    pub fn new(cert_bytes: &[u8], key_bytes: &[u8]) -> reqwest::Result<Self> {
        Self::with_base_url("https://localhost:8555".to_string(), cert_bytes, key_bytes)
    }

    /// Creates a client for the default local node with opt-in [`ClientOptions`].
    pub fn new_with_options(
        cert_bytes: &[u8],
        key_bytes: &[u8],
        options: ClientOptions,
    ) -> reqwest::Result<Self> {
        Self::with_options(
            "https://localhost:8555".to_string(),
            cert_bytes,
            key_bytes,
            options,
        )
    }

    pub fn with_base_url(
        base_url: String,
        cert_bytes: &[u8],
        key_bytes: &[u8],
    ) -> reqwest::Result<Self> {
        Self::with_options(base_url, cert_bytes, key_bytes, ClientOptions::default())
    }

    /// Creates a client with opt-in [`ClientOptions`] (e.g. request timeouts).
    pub fn with_options(
        base_url: String,
        cert_bytes: &[u8],
        key_bytes: &[u8],
        options: ClientOptions,
    ) -> reqwest::Result<Self> {
        #[cfg(feature = "native-tls")]
        let identity = Identity::from_pkcs8_pem(cert_bytes, key_bytes)?;

        #[cfg(not(feature = "native-tls"))] // rustls
        let identity = Identity::from_pem(&[key_bytes, cert_bytes].concat())?;

        Ok(Self {
            base_url,
            client: options
                .apply(
                    Client::builder()
                        .danger_accept_invalid_certs(true)
                        .identity(identity),
                )
                .build()?,
        })
    }
}

impl ChiaRpcClient for FullNodeClient {
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
