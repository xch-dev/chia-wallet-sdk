use chia::protocol::{BlockRecord, Bytes32, CoinSpend, FullBlock};
use serde::Deserialize;

use super::{
    de::{
        deserialize_block_record, deserialize_block_record_maybe, deserialize_block_records_maybe,
        deserialize_coin_spend_maybe, deserialize_coin_spends_maybe, deserialize_full_block_maybe,
        deserialize_full_blocks_maybe, hex_string_to_bytes32, hex_string_to_bytes32_maybe,
    },
    CoinRecord, DeserializableMempoolItem,
};

#[derive(Deserialize, Debug)]
pub struct BlockchainStateResponse {
    pub blockchain_state: Option<BlockchainState>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug)]
pub struct BlockchainState {
    pub average_block_time: u64,
    pub block_max_cost: u64,
    pub difficulty: u64,
    pub genesis_challenge_initialized: bool,
    pub mempool_cost: u64,
    pub mempool_fees: u64,
    pub mempool_max_total_cost: u64,
    pub mempool_min_fees: MempoolMinFees,
    pub mempool_size: u32,
    #[serde(with = "hex_string_to_bytes32")]
    pub node_id: Bytes32,
    #[serde(with = "deserialize_block_record")]
    pub peak: BlockRecord,
    pub space: u128,
    pub sub_slot_iters: u64,
    pub sync: Sync,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MempoolMinFees {
    pub cost_5000000: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Sync {
    pub sync_mode: bool,
    pub sync_progress_height: u32,
    pub sync_tip_height: u32,
    pub synced: bool,
}

#[derive(Deserialize, Debug)]
pub struct AdditionsAndRemovalsResponse {
    pub additions: Option<Vec<CoinRecord>>,
    pub removals: Option<Vec<CoinRecord>>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug)]
pub struct GetBlockResponse {
    #[serde(with = "deserialize_full_block_maybe")]
    pub block: Option<FullBlock>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug)]
pub struct GetBlockRecordResponse {
    #[serde(with = "deserialize_block_record_maybe")]
    pub block_record: Option<BlockRecord>,
    pub error: Option<String>,
    pub success: bool,
}

pub type GetBlockRecordByHeightResponse = GetBlockRecordResponse;

#[derive(Deserialize, Debug)]
pub struct GetBlockRecordsResponse {
    #[serde(with = "deserialize_block_records_maybe")]
    pub block_records: Option<Vec<BlockRecord>>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug)]
pub struct GetBlocksResponse {
    #[serde(with = "deserialize_full_blocks_maybe")]
    pub blocks: Option<Vec<FullBlock>>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug)]
pub struct GetBlockSpendsResponse {
    #[serde(with = "deserialize_coin_spends_maybe")]
    pub block_spends: Option<Vec<CoinSpend>>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug)]
pub struct GetCoinRecordResponse {
    pub coin_record: Option<CoinRecord>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug)]
pub struct GetCoinRecordsResponse {
    pub coin_records: Option<Vec<CoinRecord>>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug)]
pub struct GetPuzzleAndSolutionResponse {
    #[serde(with = "deserialize_coin_spend_maybe")]
    pub coin_solution: Option<CoinSpend>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug)]
pub struct PushTxResponse {
    pub status: String,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug)]
pub struct GetNetworkInfoResponse {
    pub network_name: Option<String>,
    pub network_prefix: Option<String>,
    #[serde(with = "hex_string_to_bytes32_maybe")]
    pub genesis_challenge: Option<Bytes32>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize)]
pub struct GetMempoolItemResponse {
    pub mempool_item: Option<DeserializableMempoolItem>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize)]
pub struct GetMempoolItemsResponse {
    pub mempool_items: Option<Vec<DeserializableMempoolItem>>,
    pub error: Option<String>,
    pub success: bool,
}
