use std::{net::IpAddr, time::Duration};

use reqwest::{Certificate, Client, ClientBuilder, Identity, Url};
use serde::{Serialize, de::DeserializeOwned};

use crate::{ChiaRpcClient, DEFAULT_RPC_TIMEOUT, coinset_client::post_json};

#[derive(Debug)]
pub struct FullNodeClient {
    base_url: String,
    client: Client,
    timeout: Duration,
}

impl FullNodeClient {
    pub fn new(cert_bytes: &[u8], key_bytes: &[u8]) -> reqwest::Result<Self> {
        Self::with_base_url("https://localhost:8555".to_string(), cert_bytes, key_bytes)
    }

    /// Connects to a full node RPC server using its client certificate and key.
    ///
    /// The full node's certificate is signed by its own private CA, which isn't known here.
    /// It's only accepted without verification if the host is loopback (such as `localhost`).
    /// To connect to a remote full node, use [`FullNodeClient::with_ca_cert`] instead.
    pub fn with_base_url(
        base_url: String,
        cert_bytes: &[u8],
        key_bytes: &[u8],
    ) -> reqwest::Result<Self> {
        let accept_invalid_certs = is_loopback(&base_url);

        let client = builder(cert_bytes, key_bytes)?
            .tls_danger_accept_invalid_certs(accept_invalid_certs)
            .build()?;

        Ok(Self {
            base_url,
            client,
            timeout: DEFAULT_RPC_TIMEOUT,
        })
    }

    /// Connects to a full node RPC server, verifying that its certificate is signed by the
    /// given CA certificate (typically `config/ssl/ca/private_ca.crt`).
    ///
    /// The hostname isn't verified, since Chia's certificates aren't issued for the node's host.
    pub fn with_ca_cert(
        base_url: String,
        cert_bytes: &[u8],
        key_bytes: &[u8],
        ca_cert_bytes: &[u8],
    ) -> reqwest::Result<Self> {
        let client = builder(cert_bytes, key_bytes)?
            .tls_certs_only([Certificate::from_pem(ca_cert_bytes)?])
            .tls_danger_accept_invalid_hostnames(true)
            .build()?;

        Ok(Self {
            base_url,
            client,
            timeout: DEFAULT_RPC_TIMEOUT,
        })
    }

    /// Sets how long to wait for each request to complete.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

fn builder(cert_bytes: &[u8], key_bytes: &[u8]) -> reqwest::Result<ClientBuilder> {
    #[cfg(feature = "native-tls")]
    let identity = Identity::from_pkcs8_pem(cert_bytes, key_bytes)?;

    #[cfg(not(feature = "native-tls"))] // rustls
    let identity = Identity::from_pem(&[key_bytes, cert_bytes].concat())?;

    Ok(Client::builder()
        .identity(identity)
        .connect_timeout(Duration::from_secs(10)))
}

fn is_loopback(base_url: &str) -> bool {
    let Ok(url) = Url::parse(base_url) else {
        return false;
    };

    let Some(host) = url.host_str() else {
        return false;
    };

    let host = host.trim_start_matches('[').trim_end_matches(']');

    host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
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
        post_json(&self.client, &url, self.timeout, &body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_loopback() {
        assert!(is_loopback("https://localhost:8555"));
        assert!(is_loopback("https://LOCALHOST:8555/"));
        assert!(is_loopback("https://127.0.0.1:8555"));
        assert!(is_loopback("https://127.1.2.3:8555"));
        assert!(is_loopback("https://[::1]:8555"));

        assert!(!is_loopback("https://example.com:8555"));
        assert!(!is_loopback("https://localhost.example.com:8555"));
        assert!(!is_loopback("https://192.168.1.10:8555"));
        assert!(!is_loopback("not a url"));
    }
}
