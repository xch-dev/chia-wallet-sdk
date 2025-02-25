use reqwest::{Client, Identity};
use serde::{de::DeserializeOwned, Serialize};

use crate::ChiaRpcClient;

#[derive(Debug)]
pub struct SslClient {
    base_url: String,
    client: Client,
}

impl SslClient {
    pub fn new(cert_bytes: &[u8], key_bytes: &[u8]) -> Self {
        #[cfg(feature = "native-tls")]
        let identity = Identity::from_pkcs8_pem(cert_bytes, key_bytes).unwrap();

        #[cfg(feature = "rustls")]
        let identity = Identity::from_pem(&[key_bytes, cert_bytes].concat()).unwrap();

        Self {
            base_url: "https://localhost:8555".to_string(),
            client: Client::builder()
                .danger_accept_invalid_certs(true)
                .identity(identity)
                .build()
                .unwrap(),
        }
    }
}

impl ChiaRpcClient for SslClient {
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
