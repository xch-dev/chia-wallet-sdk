use std::sync::Arc;

use bindy::Result;
use chia_protocol::{Bytes32, SpendBundle};
use chia_sdk_coinset::{
    AdditionsAndRemovalsResponse, BlockchainStateResponse, ChiaRpcClient,
    GetBlockRecordByHeightResponse, GetBlockRecordResponse, GetBlockRecordsResponse,
    GetBlockResponse, GetBlockSpendsResponse, GetBlocksResponse, GetCoinRecordResponse,
    GetCoinRecordsResponse, GetMempoolItemResponse, GetMempoolItemsResponse,
    GetNetworkInfoResponse, GetPuzzleAndSolutionResponse, PushTxResponse,
};
use serde::{Serialize, de::DeserializeOwned};

use crate::runtime::ms_to_duration;

#[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
use chia_protocol::Bytes;

enum RpcClientImpl {
    Coinset(chia_sdk_coinset::CoinsetClient),
    #[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
    FullNode(chia_sdk_coinset::FullNodeClient),
}

impl ChiaRpcClient for RpcClientImpl {
    type Error = reqwest::Error;

    fn base_url(&self) -> &str {
        match self {
            RpcClientImpl::Coinset(client) => client.base_url(),
            #[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
            RpcClientImpl::FullNode(client) => client.base_url(),
        }
    }

    async fn make_post_request<R, B>(
        &self,
        endpoint: &str,
        body: B,
    ) -> std::result::Result<R, Self::Error>
    where
        B: Serialize + Send,
        R: DeserializeOwned + Send,
    {
        match self {
            RpcClientImpl::Coinset(client) => client.make_post_request(endpoint, body).await,
            #[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
            RpcClientImpl::FullNode(client) => client.make_post_request(endpoint, body).await,
        }
    }
}

/// Opt-in timeout configuration for an [`RpcClient`]. All values are in milliseconds.
///
/// `request_timeout_ms` is the whole-request budget for one HTTP call (connect + send +
/// receive); `None` leaves requests unbounded. `connect_timeout_ms` bounds just the
/// connection phase; `None` falls back to the OS-level TCP connect timeout, not
/// "unbounded." Field naming mirrors [`PeerOptions`] for consistency across binding APIs.
#[derive(Clone, Default)]
pub struct RpcClientOptions {
    pub request_timeout_ms: Option<u32>,
    pub connect_timeout_ms: Option<u32>,
}

impl RpcClientOptions {
    fn to_client_options(&self) -> chia_sdk_coinset::ClientOptions {
        chia_sdk_coinset::ClientOptions {
            timeout: ms_to_duration(self.request_timeout_ms),
            connect_timeout: ms_to_duration(self.connect_timeout_ms),
        }
    }
}

// Construction params retained on the client so `with_options` can rebuild the
// underlying reqwest Client (cert/key bytes are consumed into `reqwest::Identity`
// and cannot be recovered from a built FullNodeClient).
enum ClientConfig {
    Coinset {
        base_url: String,
    },
    #[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
    FullNode {
        base_url: String,
        cert_bytes: Vec<u8>,
        key_bytes: Vec<u8>,
    },
}

#[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
const LOCAL_FULL_NODE_URL: &str = "https://localhost:8555";

#[derive(Clone)]
pub struct RpcClient {
    inner: Arc<RpcClientImpl>,
    config: Arc<ClientConfig>,
}

impl RpcClient {
    pub fn new(coinset_url: String) -> Result<Self> {
        Self::from_coinset(coinset_url, &RpcClientOptions::default())
    }

    pub fn testnet11() -> Result<Self> {
        Self::from_coinset(
            "https://testnet11.api.coinset.org".to_string(),
            &RpcClientOptions::default(),
        )
    }

    pub fn mainnet() -> Result<Self> {
        Self::from_coinset(
            "https://api.coinset.org".to_string(),
            &RpcClientOptions::default(),
        )
    }

    #[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
    pub fn local(cert_bytes: Bytes, key_bytes: Bytes) -> Result<Self> {
        Self::from_full_node(
            LOCAL_FULL_NODE_URL.to_string(),
            cert_bytes.to_vec(),
            key_bytes.to_vec(),
            &RpcClientOptions::default(),
        )
    }

    #[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
    pub fn local_with_url(base_url: String, cert_bytes: Bytes, key_bytes: Bytes) -> Result<Self> {
        Self::from_full_node(
            base_url,
            cert_bytes.to_vec(),
            key_bytes.to_vec(),
            &RpcClientOptions::default(),
        )
    }

    /// Returns a new [`RpcClient`] reconfigured with the given options, preserving
    /// the original endpoint (and, for local clients, the cert/key).
    pub fn with_options(&self, options: RpcClientOptions) -> Result<Self> {
        match self.config.as_ref() {
            ClientConfig::Coinset { base_url } => Self::from_coinset(base_url.clone(), &options),
            #[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
            ClientConfig::FullNode {
                base_url,
                cert_bytes,
                key_bytes,
            } => Self::from_full_node(
                base_url.clone(),
                cert_bytes.clone(),
                key_bytes.clone(),
                &options,
            ),
        }
    }

    fn from_coinset(base_url: String, options: &RpcClientOptions) -> Result<Self> {
        let client = chia_sdk_coinset::CoinsetClient::with_options(
            base_url.clone(),
            options.to_client_options(),
        )?;
        Ok(Self {
            inner: Arc::new(RpcClientImpl::Coinset(client)),
            config: Arc::new(ClientConfig::Coinset { base_url }),
        })
    }

    #[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
    fn from_full_node(
        base_url: String,
        cert_bytes: Vec<u8>,
        key_bytes: Vec<u8>,
        options: &RpcClientOptions,
    ) -> Result<Self> {
        let client = chia_sdk_coinset::FullNodeClient::with_options(
            base_url.clone(),
            &cert_bytes,
            &key_bytes,
            options.to_client_options(),
        )?;
        Ok(Self {
            inner: Arc::new(RpcClientImpl::FullNode(client)),
            config: Arc::new(ClientConfig::FullNode {
                base_url,
                cert_bytes,
                key_bytes,
            }),
        })
    }

    pub async fn get_blockchain_state(&self) -> Result<BlockchainStateResponse> {
        Ok(self.inner.get_blockchain_state().await?)
    }

    pub async fn get_additions_and_removals(
        &self,
        header_hash: Bytes32,
    ) -> Result<AdditionsAndRemovalsResponse> {
        Ok(self.inner.get_additions_and_removals(header_hash).await?)
    }

    pub async fn get_block(&self, header_hash: Bytes32) -> Result<GetBlockResponse> {
        Ok(self.inner.get_block(header_hash).await?)
    }

    pub async fn get_block_record(&self, header_hash: Bytes32) -> Result<GetBlockRecordResponse> {
        Ok(self.inner.get_block_record(header_hash).await?)
    }

    pub async fn get_block_record_by_height(
        &self,
        height: u32,
    ) -> Result<GetBlockRecordByHeightResponse> {
        Ok(self.inner.get_block_record_by_height(height).await?)
    }

    pub async fn get_block_records(&self, start: u32, end: u32) -> Result<GetBlockRecordsResponse> {
        Ok(self.inner.get_block_records(start, end).await?)
    }

    pub async fn get_blocks(
        &self,
        start: u32,
        end: u32,
        exclude_header_hash: bool,
        exclude_reorged: bool,
    ) -> Result<GetBlocksResponse> {
        Ok(self
            .inner
            .get_blocks(start, end, exclude_header_hash, exclude_reorged)
            .await?)
    }

    pub async fn get_block_spends(&self, header_hash: Bytes32) -> Result<GetBlockSpendsResponse> {
        Ok(self.inner.get_block_spends(header_hash).await?)
    }

    pub async fn get_coin_record_by_name(&self, name: Bytes32) -> Result<GetCoinRecordResponse> {
        Ok(self.inner.get_coin_record_by_name(name).await?)
    }

    pub async fn get_coin_records_by_hint(
        &self,
        hint: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
        cursor: Option<String>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self
            .inner
            .get_coin_records_by_hint(
                hint,
                start_height,
                end_height,
                include_spent_coins,
                cursor,
            )
            .await?)
    }

    pub async fn get_coin_records_by_hints(
        &self,
        hints: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
        cursor: Option<String>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self
            .inner
            .get_coin_records_by_hints(
                hints,
                start_height,
                end_height,
                include_spent_coins,
                cursor,
            )
            .await?)
    }

    pub async fn get_coin_records_by_names(
        &self,
        names: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
        cursor: Option<String>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self
            .inner
            .get_coin_records_by_names(
                names,
                start_height,
                end_height,
                include_spent_coins,
                cursor,
            )
            .await?)
    }

    pub async fn get_coin_records_by_parent_ids(
        &self,
        parent_ids: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
        cursor: Option<String>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self
            .inner
            .get_coin_records_by_parent_ids(
                parent_ids,
                start_height,
                end_height,
                include_spent_coins,
                cursor,
            )
            .await?)
    }

    pub async fn get_coin_records_by_puzzle_hash(
        &self,
        puzzle_hash: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
        cursor: Option<String>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self
            .inner
            .get_coin_records_by_puzzle_hash(
                puzzle_hash,
                start_height,
                end_height,
                include_spent_coins,
                cursor,
            )
            .await?)
    }

    pub async fn get_coin_records_by_puzzle_hashes(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
        cursor: Option<String>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self
            .inner
            .get_coin_records_by_puzzle_hashes(
                puzzle_hashes,
                start_height,
                end_height,
                include_spent_coins,
                cursor,
            )
            .await?)
    }

    pub async fn get_puzzle_and_solution(
        &self,
        coin_id: Bytes32,
        height: Option<u32>,
    ) -> Result<GetPuzzleAndSolutionResponse> {
        Ok(self.inner.get_puzzle_and_solution(coin_id, height).await?)
    }

    pub async fn push_tx(&self, spend_bundle: SpendBundle) -> Result<PushTxResponse> {
        Ok(self.inner.push_tx(spend_bundle).await?)
    }

    pub async fn get_network_info(&self) -> Result<GetNetworkInfoResponse> {
        Ok(self.inner.get_network_info().await?)
    }

    pub async fn get_mempool_item_by_tx_id(
        &self,
        tx_id: Bytes32,
    ) -> Result<GetMempoolItemResponse> {
        Ok(self.inner.get_mempool_item_by_tx_id(tx_id).await?)
    }

    pub async fn get_mempool_items_by_coin_name(
        &self,
        coin_name: Bytes32,
    ) -> Result<GetMempoolItemsResponse> {
        Ok(self.inner.get_mempool_items_by_coin_name(coin_name).await?)
    }
}
