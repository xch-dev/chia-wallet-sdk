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

#[derive(Clone)]
pub struct CoinsetClient(Arc<chia_sdk_coinset::CoinsetClient>);

impl CoinsetClient {
    pub fn new(base_url: String) -> Result<Self> {
        Ok(Self(Arc::new(chia_sdk_coinset::CoinsetClient::new(
            base_url,
        ))))
    }

    pub fn testnet11() -> Result<Self> {
        Ok(Self(Arc::new(chia_sdk_coinset::CoinsetClient::testnet11())))
    }

    pub fn mainnet() -> Result<Self> {
        Ok(Self(Arc::new(chia_sdk_coinset::CoinsetClient::mainnet())))
    }

    pub async fn get_blockchain_state(&self) -> Result<BlockchainStateResponse> {
        Ok(self.0.get_blockchain_state().await?)
    }

    pub async fn get_additions_and_removals(
        &self,
        header_hash: Bytes32,
    ) -> Result<AdditionsAndRemovalsResponse> {
        Ok(self.0.get_additions_and_removals(header_hash).await?)
    }

    pub async fn get_block(&self, header_hash: Bytes32) -> Result<GetBlockResponse> {
        Ok(self.0.get_block(header_hash).await?)
    }

    pub async fn get_block_record(&self, header_hash: Bytes32) -> Result<GetBlockRecordResponse> {
        Ok(self.0.get_block_record(header_hash).await?)
    }

    pub async fn get_block_record_by_height(
        &self,
        height: u32,
    ) -> Result<GetBlockRecordByHeightResponse> {
        Ok(self.0.get_block_record_by_height(height).await?)
    }

    pub async fn get_block_records(
        &self,
        start_height: u32,
        end_height: u32,
    ) -> Result<GetBlockRecordsResponse> {
        Ok(self.0.get_block_records(start_height, end_height).await?)
    }

    pub async fn get_blocks(
        &self,
        start: u32,
        end: u32,
        exclude_header_hash: bool,
        exclude_reorged: bool,
    ) -> Result<GetBlocksResponse> {
        Ok(self
            .0
            .get_blocks(start, end, exclude_header_hash, exclude_reorged)
            .await?)
    }

    pub async fn get_block_spends(&self, header_hash: Bytes32) -> Result<GetBlockSpendsResponse> {
        Ok(self.0.get_block_spends(header_hash).await?)
    }

    pub async fn get_coin_record_by_name(&self, name: Bytes32) -> Result<GetCoinRecordResponse> {
        Ok(self.0.get_coin_record_by_name(name).await?)
    }

    pub async fn get_coin_records_by_hint(
        &self,
        hint: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self
            .0
            .get_coin_records_by_hint(hint, start_height, end_height, include_spent_coins)
            .await?)
    }

    pub async fn get_coin_records_by_names(
        &self,
        names: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self
            .0
            .get_coin_records_by_names(names, start_height, end_height, include_spent_coins)
            .await?)
    }

    pub async fn get_coin_records_by_parent_ids(
        &self,
        parent_ids: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self
            .0
            .get_coin_records_by_parent_ids(
                parent_ids,
                start_height,
                end_height,
                include_spent_coins,
            )
            .await?)
    }

    pub async fn get_coin_records_by_puzzle_hash(
        &self,
        puzzle_hash: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self
            .0
            .get_coin_records_by_puzzle_hash(
                puzzle_hash,
                start_height,
                end_height,
                include_spent_coins,
            )
            .await?)
    }

    pub async fn get_coin_records_by_puzzle_hashes(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self
            .0
            .get_coin_records_by_puzzle_hashes(
                puzzle_hashes,
                start_height,
                end_height,
                include_spent_coins,
            )
            .await?)
    }

    pub async fn get_puzzle_and_solution(
        &self,
        coin_id: Bytes32,
        height: Option<u32>,
    ) -> Result<GetPuzzleAndSolutionResponse> {
        Ok(self.0.get_puzzle_and_solution(coin_id, height).await?)
    }

    pub async fn push_tx(&self, spend_bundle: SpendBundle) -> Result<PushTxResponse> {
        Ok(self.0.push_tx(spend_bundle).await?)
    }

    pub async fn get_network_info(&self) -> Result<GetNetworkInfoResponse> {
        Ok(self.0.get_network_info().await?)
    }

    pub async fn get_mempool_item_by_tx_id(
        &self,
        tx_id: Bytes32,
    ) -> Result<GetMempoolItemResponse> {
        Ok(self.0.get_mempool_item_by_tx_id(tx_id).await?)
    }

    pub async fn get_mempool_items_by_coin_name(
        &self,
        coin_name: Bytes32,
    ) -> Result<GetMempoolItemsResponse> {
        Ok(self.0.get_mempool_items_by_coin_name(coin_name).await?)
    }
}
