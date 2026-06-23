use chia_protocol::{BlockRecord, Bytes32, Coin, SpendBundle};
use serde::{Deserialize, Serialize};

use crate::FullNodeSimulatorEvent;

#[derive(Debug, Deserialize)]
pub(super) struct EmptyRequest {}

#[derive(Debug, Deserialize)]
pub(super) struct HeaderHashRequest {
    pub(super) header_hash: Bytes32,
}

#[derive(Debug, Deserialize)]
pub(super) struct HeightRequest {
    pub(super) height: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct BlockRecordsRequest {
    pub(super) start: u32,
    pub(super) end: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct BlocksRequest {
    #[allow(dead_code)]
    pub(super) start: u32,
    #[allow(dead_code)]
    pub(super) end: u32,
    #[allow(dead_code)]
    pub(super) exclude_header_hash: bool,
    #[allow(dead_code)]
    pub(super) exclude_reorged: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct NameRequest {
    pub(super) name: Bytes32,
}

#[derive(Debug, Deserialize)]
pub(super) struct NamesRequest {
    pub(super) names: Vec<Bytes32>,
    pub(super) start_height: Option<u32>,
    pub(super) end_height: Option<u32>,
    pub(super) include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    pub(super) cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct HintRequest {
    pub(super) hint: Bytes32,
    pub(super) start_height: Option<u32>,
    pub(super) end_height: Option<u32>,
    pub(super) include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    pub(super) cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct HintsRequest {
    pub(super) hints: Vec<Bytes32>,
    pub(super) start_height: Option<u32>,
    pub(super) end_height: Option<u32>,
    pub(super) include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    pub(super) cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ParentIdsRequest {
    pub(super) parent_ids: Vec<Bytes32>,
    pub(super) start_height: Option<u32>,
    pub(super) end_height: Option<u32>,
    pub(super) include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    pub(super) cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PuzzleHashRequest {
    pub(super) puzzle_hash: Bytes32,
    pub(super) start_height: Option<u32>,
    pub(super) end_height: Option<u32>,
    pub(super) include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    pub(super) cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PuzzleHashesRequest {
    pub(super) puzzle_hashes: Vec<Bytes32>,
    pub(super) start_height: Option<u32>,
    pub(super) end_height: Option<u32>,
    pub(super) include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    pub(super) cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PuzzleAndSolutionRequest {
    pub(super) coin_id: Bytes32,
    pub(super) height: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PushTxRequest {
    pub(super) spend_bundle: SpendBundle,
}

#[derive(Debug, Deserialize)]
pub(super) struct TxIdRequest {
    pub(super) tx_id: Bytes32,
}

#[derive(Debug, Deserialize)]
pub(super) struct CoinNameRequest {
    pub(super) coin_name: Bytes32,
}

#[derive(Debug, Deserialize)]
pub(super) struct FarmBlockRequest {
    pub(super) blocks: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct RevertBlocksRequest {
    pub(super) blocks: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct ReorgBlocksRequest {
    pub(super) num_of_blocks_to_rev: u32,
    pub(super) num_of_new_blocks: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct NewCoinRequest {
    pub(super) puzzle_hash: Bytes32,
    pub(super) amount: u64,
}

#[derive(Debug, Deserialize)]
pub(super) struct InsertCoinRequest {
    pub(super) coin: Coin,
}

#[derive(Debug, Deserialize)]
pub(super) struct SetFarmingPhRequest {
    pub(super) puzzle_hash: Bytes32,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct SimSuccessResponse {
    pub(super) success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct SimFarmBlockResponse {
    pub(super) block_records: Vec<BlockRecord>,
    pub(super) success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct SimRevertBlocksResponse {
    pub(super) header_hashes: Vec<Bytes32>,
    pub(super) success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct SimNewCoinResponse {
    pub(super) coin: Coin,
    pub(super) success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct SimEventsResponse {
    pub(super) events: Vec<FullNodeSimulatorEvent>,
    pub(super) success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GetAggsigAdditionalDataResponse {
    pub(super) additional_data: Option<String>,
    pub(super) error: Option<String>,
    pub(super) success: bool,
}
