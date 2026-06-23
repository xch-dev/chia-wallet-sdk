use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_sdk_coinset::{ChiaRpcClient, CoinsetClient, PushTxResponse};
use chia_sdk_types::conditions::{CreateCoin, Memos};
use clvmr::NodePtr;

use crate::{to_program, to_puzzle};

use super::{
    push_tx::push_tx_response_body,
    server::FullNodeSimulatorServer,
    types::{
        GetAggsigAdditionalDataResponse, SimFarmBlockResponse, SimNewCoinResponse,
        SimSuccessResponse,
    },
};

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
async fn get_coin_records_by_puzzle_hashes_defaults_to_historical_outputs() -> anyhow::Result<()> {
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
