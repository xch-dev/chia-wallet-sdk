use chia_protocol::{BlockRecord, Bytes32, CoinSpend, FullBlock};
use serde::{Deserialize, Serialize};

use crate::{CoinRecord, MempoolItem};

/// The response to [`ChiaRpcClient::get_blockchain_state`](crate::ChiaRpcClient::get_blockchain_state).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlockchainStateResponse {
    /// The state of the blockchain, if the request succeeded.
    pub blockchain_state: Option<BlockchainState>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The node's view of the blockchain and mempool.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlockchainState {
    /// The average time between recent transaction blocks, in seconds.
    pub average_block_time: u64,
    /// The maximum cost of a block.
    pub block_max_cost: u64,
    /// The current difficulty.
    pub difficulty: u64,
    /// Whether the node has initialized its genesis challenge.
    pub genesis_challenge_initialized: bool,
    /// The total cost of the items in the mempool.
    pub mempool_cost: u64,
    /// The total fees of the items in the mempool, in mojos.
    pub mempool_fees: u64,
    /// The maximum total cost of the items in the mempool.
    pub mempool_max_total_cost: u64,
    /// The minimum fees required to get into the mempool.
    pub mempool_min_fees: MempoolMinFees,
    /// The number of items in the mempool.
    pub mempool_size: u32,
    /// The id of the node.
    pub node_id: Bytes32,
    /// The block record of the node's current peak.
    pub peak: BlockRecord,
    /// The estimated total netspace of the network, in bytes.
    pub space: u128,
    /// The current number of iterations per sub-slot.
    pub sub_slot_iters: u64,
    /// The sync status of the node.
    pub sync: SyncState,
}

/// The minimum fees required to get into the mempool when it's full.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct MempoolMinFees {
    /// The minimum fee, in mojos, for a transaction with a cost of 5,000,000.
    pub cost_5000000: u64,
}

/// Whether the node is synced with the network.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct SyncState {
    /// Whether the node is currently syncing.
    pub sync_mode: bool,
    /// The height the node has synced to.
    pub sync_progress_height: u32,
    /// The height of the peak the node is syncing to.
    pub sync_tip_height: u32,
    /// Whether the node is fully synced.
    pub synced: bool,
}

/// The response to [`ChiaRpcClient::get_additions_and_removals`](crate::ChiaRpcClient::get_additions_and_removals).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AdditionsAndRemovalsResponse {
    /// The coins created in the block.
    pub additions: Option<Vec<CoinRecord>>,
    /// The coins spent in the block.
    pub removals: Option<Vec<CoinRecord>>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to [`ChiaRpcClient::get_block`](crate::ChiaRpcClient::get_block).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetBlockResponse {
    /// The block, if it was found.
    pub block: Option<FullBlock>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to [`ChiaRpcClient::get_block_record`](crate::ChiaRpcClient::get_block_record) and
/// [`ChiaRpcClient::get_block_record_by_height`](crate::ChiaRpcClient::get_block_record_by_height).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetBlockRecordResponse {
    /// The block record, if it was found.
    pub block_record: Option<BlockRecord>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to [`ChiaRpcClient::get_block_record_by_height`](crate::ChiaRpcClient::get_block_record_by_height),
/// which has the same shape as [`GetBlockRecordResponse`].
pub type GetBlockRecordByHeightResponse = GetBlockRecordResponse;

/// The response to [`ChiaRpcClient::get_block_records`](crate::ChiaRpcClient::get_block_records).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetBlockRecordsResponse {
    /// The block records in the requested range.
    pub block_records: Option<Vec<BlockRecord>>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to [`ChiaRpcClient::get_blocks`](crate::ChiaRpcClient::get_blocks).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetBlocksResponse {
    /// The blocks in the requested range.
    pub blocks: Option<Vec<FullBlock>>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to [`ChiaRpcClient::get_block_spends`](crate::ChiaRpcClient::get_block_spends).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetBlockSpendsResponse {
    /// The coin spends in the block.
    pub block_spends: Option<Vec<CoinSpend>>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to [`ChiaRpcClient::get_coin_record_by_name`](crate::ChiaRpcClient::get_coin_record_by_name).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetCoinRecordResponse {
    /// The coin record, if it was found.
    pub coin_record: Option<CoinRecord>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to the `get_coin_records_by_*` methods of [`ChiaRpcClient`](crate::ChiaRpcClient).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetCoinRecordsResponse {
    /// The coin records that matched the request.
    pub coin_records: Option<Vec<CoinRecord>>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
    /// Set to `true` by coinset when the response was truncated due to the
    /// server's `scan_max_keys_per_stream` limit. Not present in Chia full
    /// node responses.
    pub truncated: Option<bool>,
    /// Cursor for fetching the next page. Only present when `truncated`
    /// is `true`. Pass this value back in the request's `cursor` field to
    /// resume from where the previous response left off.
    pub next_cursor: Option<String>,
}

/// The response to [`ChiaRpcClient::get_puzzle_and_solution`](crate::ChiaRpcClient::get_puzzle_and_solution).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetPuzzleAndSolutionResponse {
    /// The coin spend, including the puzzle reveal and solution, if it was found.
    pub coin_solution: Option<CoinSpend>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to [`ChiaRpcClient::push_tx`](crate::ChiaRpcClient::push_tx).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PushTxResponse {
    /// The status of the transaction reported by the node, such as `SUCCESS` or `PENDING`.
    #[serde(default)]
    pub status: Option<String>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to [`ChiaRpcClient::get_network_info`](crate::ChiaRpcClient::get_network_info).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetNetworkInfoResponse {
    /// The name of the network, such as `mainnet` or `testnet11`.
    pub network_name: Option<String>,
    /// The address prefix used on the network, such as `xch` or `txch`.
    pub network_prefix: Option<String>,
    /// The genesis challenge of the network.
    pub genesis_challenge: Option<Bytes32>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to [`ChiaRpcClient::get_mempool_item_by_tx_id`](crate::ChiaRpcClient::get_mempool_item_by_tx_id).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetMempoolItemResponse {
    /// The mempool item, if it was found.
    pub mempool_item: Option<MempoolItem>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to [`ChiaRpcClient::get_mempool_items_by_coin_name`](crate::ChiaRpcClient::get_mempool_items_by_coin_name).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetMempoolItemsResponse {
    /// The mempool items that spend the coin.
    pub mempool_items: Option<Vec<MempoolItem>>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}

/// The response to [`ChiaRpcClient::get_fee_estimate`](crate::ChiaRpcClient::get_fee_estimate).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetFeeEstimateResponse {
    /// The estimated fee, in mojos, for each of the requested target times.
    #[serde(default)]
    pub estimates: Vec<u64>,
    /// The target times the estimates are for, in seconds.
    #[serde(default)]
    pub target_times: Vec<u64>,
    /// The fee rate, in mojos per cost, needed to get into the mempool right now.
    pub current_fee_rate: Option<f64>,
    /// The total cost of the transactions in the mempool.
    pub mempool_size: Option<u64>,
    /// The maximum total cost of the transactions the mempool holds.
    pub mempool_max_size: Option<u64>,
    /// Whether the node is synced.
    pub full_node_synced: Option<bool>,
    /// The height of the most recent transaction block.
    pub last_tx_block_height: Option<u32>,
    /// The error message if the request failed.
    pub error: Option<String>,
    /// Whether the request succeeded.
    pub success: bool,
}
