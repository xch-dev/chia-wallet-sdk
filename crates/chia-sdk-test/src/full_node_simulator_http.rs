use std::{
    net::SocketAddr,
    ops::Deref,
    sync::{Arc, Mutex},
};

use axum::{Json, Router, extract::State, routing::post};
use chia_protocol::{BlockRecord, Bytes32, Coin, SpendBundle};
use chia_sdk_coinset::{
    AdditionsAndRemovalsResponse, BlockchainStateResponse, GetBlockRecordResponse,
    GetBlockRecordsResponse, GetBlockResponse, GetBlockSpendsResponse, GetBlocksResponse,
    GetCoinRecordResponse, GetCoinRecordsResponse, GetMempoolItemResponse, GetMempoolItemsResponse,
    GetNetworkInfoResponse, GetPuzzleAndSolutionResponse, PushTxResponse,
};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, task::JoinHandle};

use crate::{FullNodeSimulator, FullNodeSimulatorEvent};

type SharedSimulator = Arc<Mutex<FullNodeSimulator>>;

#[derive(Debug)]
pub struct FullNodeSimulatorServer {
    addr: SocketAddr,
    simulator: SharedSimulator,
    join_handle: JoinHandle<()>,
}

impl Deref for FullNodeSimulatorServer {
    type Target = Mutex<FullNodeSimulator>;

    fn deref(&self) -> &Self::Target {
        &self.simulator
    }
}

impl FullNodeSimulatorServer {
    pub async fn new() -> std::io::Result<Self> {
        Self::with_simulator(Arc::new(Mutex::new(FullNodeSimulator::default()))).await
    }

    pub async fn with_simulator(simulator: SharedSimulator) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let app = router(simulator.clone());
        let join_handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Ok(Self {
            addr,
            simulator,
            join_handle,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for FullNodeSimulatorServer {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}

fn router(simulator: SharedSimulator) -> Router {
    Router::new()
        .route("/get_blockchain_state", post(get_blockchain_state))
        .route("/get_network_info", post(get_network_info))
        .route(
            "/get_aggsig_additional_data",
            post(get_aggsig_additional_data),
        )
        .route("/get_block", post(get_block))
        .route("/get_blocks", post(get_blocks))
        .route("/get_block_record", post(get_block_record))
        .route(
            "/get_block_record_by_height",
            post(get_block_record_by_height),
        )
        .route("/get_block_records", post(get_block_records))
        .route(
            "/get_additions_and_removals",
            post(get_additions_and_removals),
        )
        .route("/get_block_spends", post(get_block_spends))
        .route("/get_coin_record_by_name", post(get_coin_record_by_name))
        .route(
            "/get_coin_records_by_names",
            post(get_coin_records_by_names),
        )
        .route("/get_coin_records_by_hint", post(get_coin_records_by_hint))
        .route(
            "/get_coin_records_by_hints",
            post(get_coin_records_by_hints),
        )
        .route(
            "/get_coin_records_by_parent_ids",
            post(get_coin_records_by_parent_ids),
        )
        .route(
            "/get_coin_records_by_puzzle_hash",
            post(get_coin_records_by_puzzle_hash),
        )
        .route(
            "/get_coin_records_by_puzzle_hashes",
            post(get_coin_records_by_puzzle_hashes),
        )
        .route("/get_puzzle_and_solution", post(get_puzzle_and_solution))
        .route("/push_tx", post(push_tx))
        .route(
            "/get_mempool_item_by_tx_id",
            post(get_mempool_item_by_tx_id),
        )
        .route(
            "/get_mempool_items_by_coin_name",
            post(get_mempool_items_by_coin_name),
        )
        .route("/sim/farm_block", post(sim_farm_block))
        .route("/sim/revert_blocks", post(sim_revert_blocks))
        .route("/sim/reorg_blocks", post(sim_reorg_blocks))
        .route("/sim/new_coin", post(sim_new_coin))
        .route("/sim/insert_coin", post(sim_insert_coin))
        .route("/sim/set_autofarm", post(sim_set_autofarm))
        .route("/sim/set_farming_ph", post(sim_set_farming_ph))
        .route("/sim/drain_events", post(sim_drain_events))
        .with_state(simulator)
}

#[derive(Debug, Deserialize)]
struct EmptyRequest {}

#[derive(Debug, Deserialize)]
struct HeaderHashRequest {
    header_hash: Bytes32,
}

#[derive(Debug, Deserialize)]
struct HeightRequest {
    height: u32,
}

#[derive(Debug, Deserialize)]
struct BlockRecordsRequest {
    start: u32,
    end: u32,
}

#[derive(Debug, Deserialize)]
struct BlocksRequest {
    #[allow(dead_code)]
    start: u32,
    #[allow(dead_code)]
    end: u32,
    #[allow(dead_code)]
    exclude_header_hash: bool,
    #[allow(dead_code)]
    exclude_reorged: bool,
}

#[derive(Debug, Deserialize)]
struct NameRequest {
    name: Bytes32,
}

#[derive(Debug, Deserialize)]
struct NamesRequest {
    names: Vec<Bytes32>,
    start_height: Option<u32>,
    end_height: Option<u32>,
    include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HintRequest {
    hint: Bytes32,
    start_height: Option<u32>,
    end_height: Option<u32>,
    include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HintsRequest {
    hints: Vec<Bytes32>,
    start_height: Option<u32>,
    end_height: Option<u32>,
    include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ParentIdsRequest {
    parent_ids: Vec<Bytes32>,
    start_height: Option<u32>,
    end_height: Option<u32>,
    include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PuzzleHashRequest {
    puzzle_hash: Bytes32,
    start_height: Option<u32>,
    end_height: Option<u32>,
    include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PuzzleHashesRequest {
    puzzle_hashes: Vec<Bytes32>,
    start_height: Option<u32>,
    end_height: Option<u32>,
    include_spent_coins: Option<bool>,
    #[allow(dead_code)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PuzzleAndSolutionRequest {
    coin_id: Bytes32,
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PushTxRequest {
    spend_bundle: SpendBundle,
}

#[derive(Debug, Deserialize)]
struct TxIdRequest {
    tx_id: Bytes32,
}

#[derive(Debug, Deserialize)]
struct CoinNameRequest {
    coin_name: Bytes32,
}

#[derive(Debug, Deserialize)]
struct FarmBlockRequest {
    blocks: u32,
}

#[derive(Debug, Deserialize)]
struct RevertBlocksRequest {
    blocks: u32,
}

#[derive(Debug, Deserialize)]
struct ReorgBlocksRequest {
    num_of_blocks_to_rev: u32,
    num_of_new_blocks: u32,
}

#[derive(Debug, Deserialize)]
struct NewCoinRequest {
    puzzle_hash: Bytes32,
    amount: u64,
}

#[derive(Debug, Deserialize)]
struct InsertCoinRequest {
    coin: Coin,
}

#[derive(Debug, Deserialize)]
struct SetAutofarmRequest {
    autofarm: bool,
}

#[derive(Debug, Deserialize)]
struct SetFarmingPhRequest {
    puzzle_hash: Bytes32,
}

#[derive(Debug, Serialize, Deserialize)]
struct SimSuccessResponse {
    success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct SimFarmBlockResponse {
    block_records: Vec<BlockRecord>,
    success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct SimRevertBlocksResponse {
    header_hashes: Vec<Bytes32>,
    success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct SimNewCoinResponse {
    coin: Coin,
    success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct SimEventsResponse {
    events: Vec<FullNodeSimulatorEvent>,
    success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct GetAggsigAdditionalDataResponse {
    additional_data: Option<String>,
    error: Option<String>,
    success: bool,
}

async fn get_blockchain_state(
    State(simulator): State<SharedSimulator>,
    Json(_request): Json<EmptyRequest>,
) -> Json<BlockchainStateResponse> {
    Json(simulator.lock().unwrap().get_blockchain_state())
}

async fn get_network_info(
    State(simulator): State<SharedSimulator>,
    Json(_request): Json<EmptyRequest>,
) -> Json<GetNetworkInfoResponse> {
    Json(simulator.lock().unwrap().get_network_info())
}

async fn get_aggsig_additional_data(
    State(simulator): State<SharedSimulator>,
    Json(_request): Json<EmptyRequest>,
) -> Json<GetAggsigAdditionalDataResponse> {
    let additional_data = simulator
        .lock()
        .unwrap()
        .get_aggsig_additional_data()
        .to_bytes();
    Json(GetAggsigAdditionalDataResponse {
        additional_data: Some(hex::encode(additional_data)),
        error: None,
        success: true,
    })
}

async fn get_block(
    State(_simulator): State<SharedSimulator>,
    Json(_request): Json<HeaderHashRequest>,
) -> Json<GetBlockResponse> {
    Json(GetBlockResponse {
        block: None,
        error: Some("get_block is not supported by FullNodeSimulator".to_string()),
        success: false,
    })
}

async fn get_blocks(
    State(_simulator): State<SharedSimulator>,
    Json(_request): Json<BlocksRequest>,
) -> Json<GetBlocksResponse> {
    Json(GetBlocksResponse {
        blocks: None,
        error: Some("get_blocks is not supported by FullNodeSimulator".to_string()),
        success: false,
    })
}

async fn get_block_record(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<HeaderHashRequest>,
) -> Json<GetBlockRecordResponse> {
    Json(
        simulator
            .lock()
            .unwrap()
            .get_block_record(request.header_hash),
    )
}

async fn get_block_record_by_height(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<HeightRequest>,
) -> Json<GetBlockRecordResponse> {
    Json(
        simulator
            .lock()
            .unwrap()
            .get_block_record_by_height(request.height),
    )
}

async fn get_block_records(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<BlockRecordsRequest>,
) -> Json<GetBlockRecordsResponse> {
    Json(
        simulator
            .lock()
            .unwrap()
            .get_block_records(request.start, request.end),
    )
}

async fn get_additions_and_removals(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<HeaderHashRequest>,
) -> Json<AdditionsAndRemovalsResponse> {
    Json(
        simulator
            .lock()
            .unwrap()
            .get_additions_and_removals(request.header_hash),
    )
}

async fn get_block_spends(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<HeaderHashRequest>,
) -> Json<GetBlockSpendsResponse> {
    Json(
        simulator
            .lock()
            .unwrap()
            .get_block_spends(request.header_hash),
    )
}

async fn get_coin_record_by_name(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<NameRequest>,
) -> Json<GetCoinRecordResponse> {
    Json(
        simulator
            .lock()
            .unwrap()
            .get_coin_record_by_name(request.name),
    )
}

async fn get_coin_records_by_names(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<NamesRequest>,
) -> Json<GetCoinRecordsResponse> {
    Json(simulator.lock().unwrap().get_coin_records_by_names(
        request.names,
        request.start_height,
        request.end_height,
        request.include_spent_coins,
    ))
}

async fn get_coin_records_by_hint(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<HintRequest>,
) -> Json<GetCoinRecordsResponse> {
    Json(simulator.lock().unwrap().get_coin_records_by_hint(
        request.hint,
        request.start_height,
        request.end_height,
        request.include_spent_coins,
    ))
}

async fn get_coin_records_by_hints(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<HintsRequest>,
) -> Json<GetCoinRecordsResponse> {
    Json(simulator.lock().unwrap().get_coin_records_by_hints(
        request.hints,
        request.start_height,
        request.end_height,
        request.include_spent_coins,
    ))
}

async fn get_coin_records_by_parent_ids(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<ParentIdsRequest>,
) -> Json<GetCoinRecordsResponse> {
    Json(simulator.lock().unwrap().get_coin_records_by_parent_ids(
        request.parent_ids,
        request.start_height,
        request.end_height,
        request.include_spent_coins,
    ))
}

async fn get_coin_records_by_puzzle_hash(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<PuzzleHashRequest>,
) -> Json<GetCoinRecordsResponse> {
    Json(simulator.lock().unwrap().get_coin_records_by_puzzle_hash(
        request.puzzle_hash,
        request.start_height,
        request.end_height,
        Some(request.include_spent_coins.unwrap_or(true)),
    ))
}

async fn get_coin_records_by_puzzle_hashes(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<PuzzleHashesRequest>,
) -> Json<GetCoinRecordsResponse> {
    Json(simulator.lock().unwrap().get_coin_records_by_puzzle_hashes(
        request.puzzle_hashes,
        request.start_height,
        request.end_height,
        Some(request.include_spent_coins.unwrap_or(true)),
    ))
}

async fn get_puzzle_and_solution(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<PuzzleAndSolutionRequest>,
) -> Json<GetPuzzleAndSolutionResponse> {
    Json(
        simulator
            .lock()
            .unwrap()
            .get_puzzle_and_solution(request.coin_id, request.height),
    )
}

async fn push_tx(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<PushTxRequest>,
) -> Json<serde_json::Value> {
    let spend_name = request.spend_bundle.name();
    let response = simulator.lock().unwrap().push_tx(request.spend_bundle);
    Json(push_tx_response_body(spend_name, response))
}

async fn get_mempool_item_by_tx_id(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<TxIdRequest>,
) -> Json<GetMempoolItemResponse> {
    Json(
        simulator
            .lock()
            .unwrap()
            .get_mempool_item_by_tx_id(request.tx_id),
    )
}

async fn get_mempool_items_by_coin_name(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<CoinNameRequest>,
) -> Json<GetMempoolItemsResponse> {
    Json(
        simulator
            .lock()
            .unwrap()
            .get_mempool_items_by_coin_name(request.coin_name),
    )
}

fn push_tx_response_body(spend_name: Bytes32, response: PushTxResponse) -> serde_json::Value {
    if response.success {
        return serde_json::json!({
            "status": response.status,
            "error": null,
            "success": true,
        });
    }

    let error_name = response
        .error
        .as_deref()
        .and_then(push_tx_error_name)
        .unwrap_or("UNKNOWN");
    if error_name == "MEMPOOL_CONFLICT" {
        return serde_json::json!({
            "status": "PENDING",
            "error": null,
            "success": true,
        });
    }

    let message = format!("Failed to include transaction {spend_name}, error {error_name}");

    serde_json::json!({
        "success": false,
        "error": message,
        "structuredError": {
            "code": "TRANSACTION_FAILED",
            "message": "Failed to include transaction",
            "data": {
                "spend_name": spend_name.to_string(),
                "error": error_name,
            },
        },
    })
}

fn push_tx_error_name(error: &str) -> Option<&'static str> {
    let error_code = error.strip_prefix("Validation error: ")?;
    Some(match error_code {
        "CostExceeded" => "BLOCK_COST_EXCEEDS_MAX",
        "MempoolConflict" => "MEMPOOL_CONFLICT",
        "InvalidSpendBundle" => "INVALID_SPEND_BUNDLE",
        "DoubleSpend" => "DOUBLE_SPEND",
        "UnknownUnspent" => "UNKNOWN_UNSPENT",
        "BadAggregateSignature" => "BAD_AGGREGATE_SIGNATURE",
        "ReserveFeeConditionFailed" => "RESERVE_FEE_CONDITION_FAILED",
        "AssertHeightAbsoluteFailed" => "ASSERT_HEIGHT_ABSOLUTE_FAILED",
        "AssertSecondsAbsoluteFailed" => "ASSERT_SECONDS_ABSOLUTE_FAILED",
        "AssertBeforeHeightAbsoluteFailed" => "ASSERT_BEFORE_HEIGHT_ABSOLUTE_FAILED",
        "AssertBeforeSecondsAbsoluteFailed" => "ASSERT_BEFORE_SECONDS_ABSOLUTE_FAILED",
        "AssertHeightRelativeFailed" => "ASSERT_HEIGHT_RELATIVE_FAILED",
        "AssertSecondsRelativeFailed" => "ASSERT_SECONDS_RELATIVE_FAILED",
        "AssertBeforeHeightRelativeFailed" => "ASSERT_BEFORE_HEIGHT_RELATIVE_FAILED",
        "AssertBeforeSecondsRelativeFailed" => "ASSERT_BEFORE_SECONDS_RELATIVE_FAILED",
        _ => "UNKNOWN",
    })
}

async fn sim_farm_block(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<FarmBlockRequest>,
) -> Json<SimFarmBlockResponse> {
    Json(SimFarmBlockResponse {
        block_records: simulator.lock().unwrap().farm_block(request.blocks),
        success: true,
    })
}

async fn sim_revert_blocks(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<RevertBlocksRequest>,
) -> Json<SimRevertBlocksResponse> {
    Json(SimRevertBlocksResponse {
        header_hashes: simulator.lock().unwrap().revert_blocks(request.blocks),
        success: true,
    })
}

async fn sim_reorg_blocks(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<ReorgBlocksRequest>,
) -> Json<SimFarmBlockResponse> {
    Json(SimFarmBlockResponse {
        block_records: simulator
            .lock()
            .unwrap()
            .reorg_blocks(request.num_of_blocks_to_rev, request.num_of_new_blocks),
        success: true,
    })
}

async fn sim_new_coin(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<NewCoinRequest>,
) -> Json<SimNewCoinResponse> {
    Json(SimNewCoinResponse {
        coin: simulator
            .lock()
            .unwrap()
            .new_coin(request.puzzle_hash, request.amount),
        success: true,
    })
}

async fn sim_insert_coin(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<InsertCoinRequest>,
) -> Json<SimSuccessResponse> {
    simulator.lock().unwrap().insert_coin(request.coin);
    Json(SimSuccessResponse { success: true })
}

async fn sim_set_autofarm(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<SetAutofarmRequest>,
) -> Json<SimSuccessResponse> {
    simulator.lock().unwrap().set_autofarm(request.autofarm);
    Json(SimSuccessResponse { success: true })
}

async fn sim_set_farming_ph(
    State(simulator): State<SharedSimulator>,
    Json(request): Json<SetFarmingPhRequest>,
) -> Json<SimSuccessResponse> {
    simulator
        .lock()
        .unwrap()
        .set_farming_ph(request.puzzle_hash);
    Json(SimSuccessResponse { success: true })
}

async fn sim_drain_events(
    State(simulator): State<SharedSimulator>,
    Json(_request): Json<EmptyRequest>,
) -> Json<SimEventsResponse> {
    Json(SimEventsResponse {
        events: simulator.lock().unwrap().drain_events(),
        success: true,
    })
}

#[cfg(test)]
mod tests {
    use chia_bls::Signature;
    use chia_protocol::{CoinSpend, SpendBundle};
    use chia_sdk_coinset::{ChiaRpcClient, CoinsetClient};
    use chia_sdk_types::conditions::{CreateCoin, Memos};
    use clvmr::NodePtr;

    use crate::{to_program, to_puzzle};

    use super::*;

    #[tokio::test]
    async fn rpc_client_can_drive_http_simulator() -> anyhow::Result<()> {
        let server = FullNodeSimulatorServer::new().await?;
        let client = CoinsetClient::new(server.url());

        let network_info = client.get_network_info().await?;
        assert!(network_info.success);
        assert_eq!(network_info.network_name.as_deref(), Some("simulator0"));

        let state = client.get_blockchain_state().await?;
        assert!(state.success);
        let blockchain_state = state.blockchain_state.unwrap();
        assert_eq!(blockchain_state.mempool_size, 0);
        let peak_hash = blockchain_state.peak.header_hash;

        let http = reqwest::Client::new();
        let aggsig_response = http
            .post(format!("{}/get_aggsig_additional_data", server.url()))
            .json(&serde_json::json!({}))
            .send()
            .await?
            .json::<GetAggsigAdditionalDataResponse>()
            .await?;
        assert!(aggsig_response.success);
        let additional_data = aggsig_response.additional_data.unwrap();
        assert_eq!(additional_data.len(), 64);
        assert!(!additional_data.starts_with("0x"));

        let response = http
            .post(format!("{}/sim/set_autofarm", server.url()))
            .json(&serde_json::json!({ "autofarm": false }))
            .send()
            .await?
            .json::<SimSuccessResponse>()
            .await?;
        assert!(response.success);

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
        let new_coin = http
            .post(format!("{}/sim/new_coin", server.url()))
            .json(&serde_json::json!({
                "puzzle_hash": puzzle_hash,
                "amount": 100_u64,
            }))
            .send()
            .await?
            .json::<SimNewCoinResponse>()
            .await?
            .coin;

        for (endpoint, body) in [
            (
                "get_coin_records_by_puzzle_hashes",
                serde_json::json!({
                    "puzzle_hashes": [puzzle_hash],
                    "start_height": null,
                    "end_height": null,
                    "include_spent_coins": null,
                }),
            ),
            (
                "get_block_record_by_height",
                serde_json::json!({ "height": 0_u32 }),
            ),
            (
                "get_coin_record_by_name",
                serde_json::json!({ "name": new_coin.coin_id() }),
            ),
            (
                "get_coin_records_by_names",
                serde_json::json!({
                    "names": [new_coin.coin_id()],
                    "start_height": null,
                    "end_height": null,
                    "include_spent_coins": null,
                }),
            ),
            (
                "get_coin_records_by_parent_ids",
                serde_json::json!({
                    "parent_ids": [new_coin.parent_coin_info],
                    "start_height": null,
                    "end_height": null,
                    "include_spent_coins": null,
                }),
            ),
            (
                "get_coin_records_by_hint",
                serde_json::json!({
                    "hint": puzzle_hash,
                    "start_height": null,
                    "end_height": null,
                    "include_spent_coins": null,
                }),
            ),
            (
                "get_puzzle_and_solution",
                serde_json::json!({
                    "coin_id": new_coin.coin_id(),
                    "height": null,
                }),
            ),
            (
                "get_block_records",
                serde_json::json!({
                    "start": 0_u32,
                    "end": 2_u32,
                }),
            ),
            (
                "get_block_spends",
                serde_json::json!({ "header_hash": peak_hash }),
            ),
        ] {
            let response = http
                .post(format!("{}/{endpoint}", server.url()))
                .json(&body)
                .send()
                .await?;
            assert!(
                response.status().is_success(),
                "{endpoint} returned {}",
                response.status()
            );
            let body = response.json::<serde_json::Value>().await?;
            assert_eq!(body.get("success"), Some(&serde_json::Value::Bool(true)));
        }

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                new_coin,
                puzzle_reveal,
                to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 99, Memos::None)])?,
            )],
            Signature::default(),
        );
        let push_response = client.push_tx(spend_bundle).await?;
        assert!(push_response.success, "{push_response:?}");

        let failed_push = http
            .post(format!("{}/push_tx", server.url()))
            .json(&serde_json::json!({
                "spend_bundle": {
                    "coin_spends": [],
                    "aggregated_signature": "0x".to_string()
                        + &hex::encode(Signature::default().to_bytes()),
                },
            }))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        assert_eq!(
            failed_push
                .get("structuredError")
                .and_then(|error| error.get("code")),
            Some(&serde_json::Value::String("TRANSACTION_FAILED".to_string()))
        );
        assert_eq!(
            failed_push
                .get("structuredError")
                .and_then(|error| error.get("data"))
                .and_then(|data| data.get("error")),
            Some(&serde_json::Value::String(
                "INVALID_SPEND_BUNDLE".to_string()
            ))
        );

        let state = client.get_blockchain_state().await?;
        assert_eq!(state.blockchain_state.unwrap().mempool_size, 1);

        let farmed = http
            .post(format!("{}/sim/farm_block", server.url()))
            .json(&serde_json::json!({ "blocks": 1_u32 }))
            .send()
            .await?
            .json::<SimFarmBlockResponse>()
            .await?;
        assert!(farmed.success);
        assert_eq!(farmed.block_records.len(), 1);

        let state = client.get_blockchain_state().await?;
        assert_eq!(state.blockchain_state.unwrap().mempool_size, 0);

        Ok(())
    }

    #[tokio::test]
    async fn get_coin_records_by_puzzle_hashes_uses_exclusive_end_height() -> anyhow::Result<()> {
        let server = FullNodeSimulatorServer::new().await?;
        let client = CoinsetClient::new(server.url());
        let http = reqwest::Client::new();
        http.post(format!("{}/sim/set_autofarm", server.url()))
            .json(&serde_json::json!({ "autofarm": false }))
            .send()
            .await?;

        let (parent_puzzle_hash, parent_puzzle_reveal) = to_puzzle(1)?;
        let (child_puzzle_hash, _) = to_puzzle(2)?;
        let parent = http
            .post(format!("{}/sim/new_coin", server.url()))
            .json(&serde_json::json!({
                "puzzle_hash": parent_puzzle_hash,
                "amount": 100_u64,
            }))
            .send()
            .await?
            .json::<SimNewCoinResponse>()
            .await?
            .coin;
        let child = Coin::new(parent.coin_id(), child_puzzle_hash, 99);
        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                parent,
                parent_puzzle_reveal,
                to_program([CreateCoin::<NodePtr>::new(
                    child_puzzle_hash,
                    child.amount,
                    Memos::None,
                )])?,
            )],
            Signature::default(),
        );

        let push_response = client.push_tx(spend_bundle).await?;
        assert!(push_response.success, "{push_response:?}");
        http.post(format!("{}/sim/farm_block", server.url()))
            .json(&serde_json::json!({ "blocks": 1_u32 }))
            .send()
            .await?;

        let before_created = client
            .get_coin_records_by_puzzle_hashes(vec![child_puzzle_hash], None, Some(2), None, None)
            .await?;
        assert_eq!(before_created.coin_records.unwrap().len(), 0);

        let after_created = client
            .get_coin_records_by_puzzle_hashes(vec![child_puzzle_hash], None, Some(3), None, None)
            .await?;
        let records = after_created.coin_records.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].coin, child);
        assert_eq!(records[0].confirmed_block_index, 2);

        Ok(())
    }

    #[tokio::test]
    async fn get_coin_records_by_puzzle_hashes_defaults_to_historical_outputs() -> anyhow::Result<()>
    {
        let server = FullNodeSimulatorServer::new().await?;
        let client = CoinsetClient::new(server.url());
        let http = reqwest::Client::new();
        http.post(format!("{}/sim/set_autofarm", server.url()))
            .json(&serde_json::json!({ "autofarm": false }))
            .send()
            .await?;

        let (historical_puzzle_hash, historical_puzzle_reveal) = to_puzzle(1)?;
        let parent = http
            .post(format!("{}/sim/new_coin", server.url()))
            .json(&serde_json::json!({
                "puzzle_hash": historical_puzzle_hash,
                "amount": 100_u64,
            }))
            .send()
            .await?
            .json::<SimNewCoinResponse>()
            .await?
            .coin;

        let historical_coin = Coin::new(parent.coin_id(), historical_puzzle_hash, 99);
        let create_historical = SpendBundle::new(
            vec![CoinSpend::new(
                parent,
                historical_puzzle_reveal.clone(),
                to_program([CreateCoin::<NodePtr>::new(
                    historical_puzzle_hash,
                    historical_coin.amount,
                    Memos::None,
                )])?,
            )],
            Signature::default(),
        );
        assert!(client.push_tx(create_historical).await?.success);
        http.post(format!("{}/sim/farm_block", server.url()))
            .json(&serde_json::json!({ "blocks": 1_u32 }))
            .send()
            .await?;

        let spend_historical = SpendBundle::new(
            vec![CoinSpend::new(
                historical_coin,
                historical_puzzle_reveal,
                to_program([CreateCoin::<NodePtr>::new(
                    historical_puzzle_hash,
                    98,
                    Memos::None,
                )])?,
            )],
            Signature::default(),
        );
        assert!(client.push_tx(spend_historical).await?.success);
        http.post(format!("{}/sim/farm_block", server.url()))
            .json(&serde_json::json!({ "blocks": 1_u32 }))
            .send()
            .await?;

        let default_records = client
            .get_coin_records_by_puzzle_hashes(vec![historical_puzzle_hash], None, None, None, None)
            .await?
            .coin_records
            .unwrap();
        assert!(
            default_records
                .iter()
                .any(|record| record.coin == historical_coin && record.spent)
        );

        let unspent_only_records = client
            .get_coin_records_by_puzzle_hashes(
                vec![historical_puzzle_hash],
                None,
                None,
                Some(false),
                None,
            )
            .await?
            .coin_records
            .unwrap();
        assert!(
            unspent_only_records
                .iter()
                .all(|record| record.coin != historical_coin)
        );

        Ok(())
    }

    #[tokio::test]
    async fn push_tx_returns_pending_for_mempool_conflict() -> anyhow::Result<()> {
        let server = FullNodeSimulatorServer::new().await?;
        let http = reqwest::Client::new();
        http.post(format!("{}/sim/set_autofarm", server.url()))
            .json(&serde_json::json!({ "autofarm": false }))
            .send()
            .await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
        let coin = http
            .post(format!("{}/sim/new_coin", server.url()))
            .json(&serde_json::json!({
                "puzzle_hash": puzzle_hash,
                "amount": 100_u64,
            }))
            .send()
            .await?
            .json::<SimNewCoinResponse>()
            .await?
            .coin;

        let first = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal.clone(),
                to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 99, Memos::None)])?,
            )],
            Signature::default(),
        );
        let conflict = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 100, Memos::None)])?,
            )],
            Signature::default(),
        );

        let first_response = http
            .post(format!("{}/push_tx", server.url()))
            .json(&serde_json::json!({ "spend_bundle": first }))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        assert_eq!(
            first_response.get("status"),
            Some(&serde_json::Value::String("SUCCESS".to_string()))
        );

        let conflict_response = http
            .post(format!("{}/push_tx", server.url()))
            .json(&serde_json::json!({ "spend_bundle": conflict }))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        assert_eq!(
            conflict_response.get("status"),
            Some(&serde_json::Value::String("PENDING".to_string()))
        );
        assert_eq!(
            conflict_response.get("success"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(conflict_response.get("structuredError").is_none());

        Ok(())
    }

    #[test]
    fn push_tx_response_maps_cost_exceeded_to_block_cost_exceeds_max() {
        let body = push_tx_response_body(
            Bytes32::default(),
            PushTxResponse {
                status: "FAILED".to_string(),
                error: Some("Validation error: CostExceeded".to_string()),
                success: false,
            },
        );

        assert_eq!(
            body.get("structuredError")
                .and_then(|error| error.get("data"))
                .and_then(|data| data.get("error")),
            Some(&serde_json::Value::String(
                "BLOCK_COST_EXCEEDS_MAX".to_string()
            ))
        );
    }
}
