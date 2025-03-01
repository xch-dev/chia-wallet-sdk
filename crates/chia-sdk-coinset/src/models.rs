use chia_protocol::{BlockRecord, Bytes32, CoinSpend, FullBlock};
use serde::Deserialize;

use crate::{CoinRecord, MempoolItem};

#[derive(Deserialize, Debug, Clone)]
pub struct BlockchainStateResponse {
    pub blockchain_state: Option<BlockchainState>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
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
    pub node_id: Bytes32,
    pub peak: BlockRecord,
    pub space: u128,
    pub sub_slot_iters: u64,
    pub sync: SyncState,
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub struct MempoolMinFees {
    pub cost_5000000: u64,
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub struct SyncState {
    pub sync_mode: bool,
    pub sync_progress_height: u32,
    pub sync_tip_height: u32,
    pub synced: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AdditionsAndRemovalsResponse {
    pub additions: Option<Vec<CoinRecord>>,
    pub removals: Option<Vec<CoinRecord>>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetBlockResponse {
    pub block: Option<FullBlock>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetBlockRecordResponse {
    pub block_record: Option<BlockRecord>,
    pub error: Option<String>,
    pub success: bool,
}

pub type GetBlockRecordByHeightResponse = GetBlockRecordResponse;

#[derive(Deserialize, Debug, Clone)]
pub struct GetBlockRecordsResponse {
    pub block_records: Option<Vec<BlockRecord>>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetBlocksResponse {
    pub blocks: Option<Vec<FullBlock>>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetBlockSpendsResponse {
    pub block_spends: Option<Vec<CoinSpend>>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetCoinRecordResponse {
    pub coin_record: Option<CoinRecord>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetCoinRecordsResponse {
    pub coin_records: Option<Vec<CoinRecord>>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetPuzzleAndSolutionResponse {
    pub coin_solution: Option<CoinSpend>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PushTxResponse {
    pub status: String,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetNetworkInfoResponse {
    pub network_name: Option<String>,
    pub network_prefix: Option<String>,
    pub genesis_challenge: Option<Bytes32>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetMempoolItemResponse {
    pub mempool_item: Option<MempoolItem>,
    pub error: Option<String>,
    pub success: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetMempoolItemsResponse {
    pub mempool_items: Option<Vec<MempoolItem>>,
    pub error: Option<String>,
    pub success: bool,
}
