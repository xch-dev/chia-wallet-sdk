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

use crate::runtime::spawn_on_runtime;

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

#[derive(Clone)]
pub struct RpcClient(Arc<RpcClientImpl>);

impl RpcClient {
    pub fn new(base_url: String) -> Result<Self> {
        Ok(Self(Arc::new(RpcClientImpl::Coinset(
            chia_sdk_coinset::CoinsetClient::new(base_url),
        ))))
    }

    pub fn testnet11() -> Result<Self> {
        Ok(Self(Arc::new(RpcClientImpl::Coinset(
            chia_sdk_coinset::CoinsetClient::testnet11(),
        ))))
    }

    pub fn mainnet() -> Result<Self> {
        Ok(Self(Arc::new(RpcClientImpl::Coinset(
            chia_sdk_coinset::CoinsetClient::mainnet(),
        ))))
    }

    #[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
    pub fn local(cert_bytes: Bytes, key_bytes: Bytes) -> Result<Self> {
        Ok(Self(Arc::new(RpcClientImpl::FullNode(
            chia_sdk_coinset::FullNodeClient::new(&cert_bytes, &key_bytes)?,
        ))))
    }

    #[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
    pub fn local_with_url(base_url: String, cert_bytes: Bytes, key_bytes: Bytes) -> Result<Self> {
        Ok(Self(Arc::new(RpcClientImpl::FullNode(
            chia_sdk_coinset::FullNodeClient::with_base_url(base_url, &cert_bytes, &key_bytes)?,
        ))))
    }

    pub async fn get_blockchain_state(&self) -> Result<BlockchainStateResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_blockchain_state().await?) }).await
    }

    pub async fn get_additions_and_removals(
        &self,
        header_hash: Bytes32,
    ) -> Result<AdditionsAndRemovalsResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_additions_and_removals(header_hash).await?) })
            .await
    }

    pub async fn get_block(&self, header_hash: Bytes32) -> Result<GetBlockResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_block(header_hash).await?) }).await
    }

    pub async fn get_block_record(&self, header_hash: Bytes32) -> Result<GetBlockRecordResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_block_record(header_hash).await?) }).await
    }

    pub async fn get_block_record_by_height(
        &self,
        height: u32,
    ) -> Result<GetBlockRecordByHeightResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_block_record_by_height(height).await?) }).await
    }

    pub async fn get_block_records(&self, start: u32, end: u32) -> Result<GetBlockRecordsResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_block_records(start, end).await?) }).await
    }

    pub async fn get_blocks(
        &self,
        start: u32,
        end: u32,
        exclude_header_hash: bool,
        exclude_reorged: bool,
    ) -> Result<GetBlocksResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move {
            Ok(client
                .get_blocks(start, end, exclude_header_hash, exclude_reorged)
                .await?)
        })
        .await
    }

    pub async fn get_block_spends(&self, header_hash: Bytes32) -> Result<GetBlockSpendsResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_block_spends(header_hash).await?) }).await
    }

    pub async fn get_coin_record_by_name(&self, name: Bytes32) -> Result<GetCoinRecordResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_coin_record_by_name(name).await?) }).await
    }

    pub async fn get_coin_records_by_hint(
        &self,
        hint: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move {
            Ok(client
                .get_coin_records_by_hint(hint, start_height, end_height, include_spent_coins)
                .await?)
        })
        .await
    }

    pub async fn get_coin_records_by_hints(
        &self,
        hints: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move {
            Ok(client
                .get_coin_records_by_hints(hints, start_height, end_height, include_spent_coins)
                .await?)
        })
        .await
    }

    pub async fn get_coin_records_by_names(
        &self,
        names: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move {
            Ok(client
                .get_coin_records_by_names(names, start_height, end_height, include_spent_coins)
                .await?)
        })
        .await
    }

    pub async fn get_coin_records_by_parent_ids(
        &self,
        parent_ids: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move {
            Ok(client
                .get_coin_records_by_parent_ids(
                    parent_ids,
                    start_height,
                    end_height,
                    include_spent_coins,
                )
                .await?)
        })
        .await
    }

    pub async fn get_coin_records_by_puzzle_hash(
        &self,
        puzzle_hash: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move {
            Ok(client
                .get_coin_records_by_puzzle_hash(
                    puzzle_hash,
                    start_height,
                    end_height,
                    include_spent_coins,
                )
                .await?)
        })
        .await
    }

    pub async fn get_coin_records_by_puzzle_hashes(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move {
            Ok(client
                .get_coin_records_by_puzzle_hashes(
                    puzzle_hashes,
                    start_height,
                    end_height,
                    include_spent_coins,
                )
                .await?)
        })
        .await
    }

    pub async fn get_puzzle_and_solution(
        &self,
        coin_id: Bytes32,
        height: Option<u32>,
    ) -> Result<GetPuzzleAndSolutionResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_puzzle_and_solution(coin_id, height).await?) })
            .await
    }

    pub async fn push_tx(&self, spend_bundle: SpendBundle) -> Result<PushTxResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.push_tx(spend_bundle).await?) }).await
    }

    pub async fn get_network_info(&self) -> Result<GetNetworkInfoResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_network_info().await?) }).await
    }

    pub async fn get_mempool_item_by_tx_id(
        &self,
        tx_id: Bytes32,
    ) -> Result<GetMempoolItemResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_mempool_item_by_tx_id(tx_id).await?) }).await
    }

    pub async fn get_mempool_items_by_coin_name(
        &self,
        coin_name: Bytes32,
    ) -> Result<GetMempoolItemsResponse> {
        let client = self.0.clone();
        spawn_on_runtime(async move { Ok(client.get_mempool_items_by_coin_name(coin_name).await?) })
            .await
    }
}
