use std::{net::SocketAddr, time::Duration};

use chia_protocol::Bytes32;
use chia_sdk_types::{MAINNET_CONSTANTS, TESTNET11_CONSTANTS};
use futures_util::{stream::FuturesUnordered, StreamExt};
use tracing::{info, instrument, warn};

use crate::ClientError;

#[derive(Debug, Clone)]
pub struct Network {
    pub default_port: u16,
    pub genesis_challenge: Bytes32,
    pub dns_introducers: Vec<String>,
}

impl Network {
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

    pub fn default_testnet11() -> Self {
        Self {
            default_port: 58444,
            genesis_challenge: TESTNET11_CONSTANTS.genesis_challenge,
            dns_introducers: vec!["dns-introducer-testnet11.chia.net".to_string()],
        }
    }

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
