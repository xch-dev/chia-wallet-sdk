use axum::{Json, Router, extract::State, routing::post};
use chia_sdk_coinset::{
    AdditionsAndRemovalsResponse, BlockchainStateResponse, GetBlockRecordResponse,
    GetBlockRecordsResponse, GetBlockResponse, GetBlockSpendsResponse, GetBlocksResponse,
    GetCoinRecordResponse, GetCoinRecordsResponse, GetMempoolItemResponse, GetMempoolItemsResponse,
    GetNetworkInfoResponse, GetPuzzleAndSolutionResponse,
};

use super::{
    push_tx::push_tx_response_body,
    state::SharedSimulator,
    types::{
        BlockRecordsRequest, BlocksRequest, CoinNameRequest, EmptyRequest, FarmBlockRequest,
        GetAggsigAdditionalDataResponse, HeaderHashRequest, HeightRequest, HintRequest,
        HintsRequest, InsertCoinRequest, NameRequest, NamesRequest, NewCoinRequest,
        ParentIdsRequest, PushTxRequest, PuzzleAndSolutionRequest, PuzzleHashRequest,
        PuzzleHashesRequest, ReorgBlocksRequest, RevertBlocksRequest, SetFarmingPhRequest,
        SimEventsResponse, SimFarmBlockResponse, SimNewCoinResponse, SimRevertBlocksResponse,
        SimSuccessResponse, TxIdRequest,
    },
};

pub(super) fn router(simulator: SharedSimulator) -> Router {
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
        .route("/sim/set_farming_ph", post(sim_set_farming_ph))
        .route("/sim/drain_events", post(sim_drain_events))
        .with_state(simulator)
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
        request.include_spent_coins,
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
        request.include_spent_coins,
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
    let response = simulator
        .lock()
        .unwrap()
        .push_tx_detailed(request.spend_bundle);
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
