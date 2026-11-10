use std::{net::SocketAddr, time::Duration};

use chia_protocol::Bytes32;
use chia_sdk_types::{MAINNET_CONSTANTS, TESTNET11_CONSTANTS};
use futures_util::{StreamExt, stream::FuturesUnordered};
use tracing::{info, instrument, warn};

use crate::ClientError;

/// Information about a Chia network that's needed to find peers on it.
///
/// # Example
///
/// ```
/// use chia_sdk_client::Network;
/// use chia_sdk_types::MAINNET_CONSTANTS;
///
/// let network = Network::default_mainnet();
/// assert_eq!(network.default_port, 8444);
/// assert_eq!(network.genesis_challenge, MAINNET_CONSTANTS.genesis_challenge);
/// ```
///
/// Peers can be found with [`Network::lookup_all`]. This example is `no_run` because it
/// makes DNS requests.
///
/// ```no_run
/// use std::time::Duration;
///
/// use chia_sdk_client::Network;
///
/// # async fn example() {
/// let network = Network::default_testnet11();
/// let peers = network.lookup_all(Duration::from_secs(3), 2).await;
/// println!("Found {} peers", peers.len());
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Network {
    /// The port that full nodes listen on, which is used for the addresses returned by
    /// DNS introducers.
    pub default_port: u16,
    /// The genesis challenge of the network.
    pub genesis_challenge: Bytes32,
    /// The hostnames of DNS introducers, which resolve to the IP addresses of full nodes.
    pub dns_introducers: Vec<String>,
}

impl Network {
    /// The settings for mainnet, which uses port 8444 and the well-known mainnet
    /// DNS introducers.
    pub fn default_mainnet() -> Self {
        Self {
            default_port: 8444,
            genesis_challenge: MAINNET_CONSTANTS.genesis_challenge,
            dns_introducers: vec![
                "dns-introducer.chia.net".to_string(),
                "chia.ctrlaltdel.ch".to_string(),
                "seeder.dexie.space".to_string(),
                "chia.hoffmang.com".to_string(),
            ],
        }
    }

    /// The settings for testnet11, which uses port 58444 and the testnet11 DNS introducer.
    pub fn default_testnet11() -> Self {
        Self {
            default_port: 58444,
            genesis_challenge: TESTNET11_CONSTANTS.genesis_challenge,
            dns_introducers: vec!["dns-introducer-testnet11.chia.net".to_string()],
        }
    }

    /// Resolves every DNS introducer and returns all of the peer addresses found.
    ///
    /// Up to `batch_size` introducers are looked up at the same time, and each lookup can take
    /// up to `timeout`. Introducers that fail or time out are logged and skipped, so this never
    /// fails, but it may return an empty list. Duplicate addresses aren't removed.
    ///
    /// # Panics
    ///
    /// Panics if `batch_size` is `0`.
    #[instrument]
    pub async fn lookup_all(&self, timeout: Duration, batch_size: usize) -> Vec<SocketAddr> {
        let mut result = Vec::new();

        for batch in self.dns_introducers.chunks(batch_size) {
            let mut futures = FuturesUnordered::new();

            for dns_introducer in batch {
                futures.push(async move {
                    match tokio::time::timeout(timeout, self.lookup_host(dns_introducer)).await {
                        Ok(Ok(addrs)) => addrs,
                        Ok(Err(error)) => {
                            warn!("Failed to lookup DNS introducer {dns_introducer}: {error}");
                            Vec::new()
                        }
                        Err(_timeout) => {
                            warn!("Timeout looking up DNS introducer {dns_introducer}");
                            Vec::new()
                        }
                    }
                });
            }

            while let Some(addrs) = futures.next().await {
                result.extend(addrs);
            }
        }

        result
    }

    /// Resolves a single DNS introducer hostname, and returns its IP addresses with the
    /// network's [`default_port`](Self::default_port).
    #[instrument]
    pub async fn lookup_host(&self, dns_introducer: &str) -> Result<Vec<SocketAddr>, ClientError> {
        info!("Looking up DNS introducer {dns_introducer}");
        let mut result = Vec::new();
        for addr in tokio::net::lookup_host(format!("{dns_introducer}:80")).await? {
            result.push(SocketAddr::new(addr.ip(), self.default_port));
        }
        Ok(result)
    }
}
