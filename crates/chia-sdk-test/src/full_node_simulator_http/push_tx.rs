use chia_consensus::validation_error::ErrorCode;
use chia_protocol::Bytes32;

use crate::{FullNodeSimulatorPushTxResponse, SimulatorError};

pub(super) fn push_tx_response_body(
    spend_name: Bytes32,
    result: FullNodeSimulatorPushTxResponse,
) -> serde_json::Value {
    if result.response.success {
        return serde_json::json!({
            "status": result.response.status,
            "error": null,
            "success": true,
        });
    }

    let error_name = result
        .error
        .as_ref()
        .map(push_tx_error_name)
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

fn push_tx_error_name(error: &SimulatorError) -> &'static str {
    let SimulatorError::Validation(error_code) = error else {
        return "UNKNOWN";
    };

    match error_code {
        ErrorCode::CostExceeded => "BLOCK_COST_EXCEEDS_MAX",
        ErrorCode::MempoolConflict => "MEMPOOL_CONFLICT",
        ErrorCode::InvalidSpendBundle => "INVALID_SPEND_BUNDLE",
        ErrorCode::DoubleSpend => "DOUBLE_SPEND",
        ErrorCode::UnknownUnspent => "UNKNOWN_UNSPENT",
        ErrorCode::BadAggregateSignature => "BAD_AGGREGATE_SIGNATURE",
        ErrorCode::ReserveFeeConditionFailed => "RESERVE_FEE_CONDITION_FAILED",
        ErrorCode::AssertHeightAbsoluteFailed => "ASSERT_HEIGHT_ABSOLUTE_FAILED",
        ErrorCode::AssertSecondsAbsoluteFailed => "ASSERT_SECONDS_ABSOLUTE_FAILED",
        ErrorCode::AssertBeforeHeightAbsoluteFailed => "ASSERT_BEFORE_HEIGHT_ABSOLUTE_FAILED",
        ErrorCode::AssertBeforeSecondsAbsoluteFailed => "ASSERT_BEFORE_SECONDS_ABSOLUTE_FAILED",
        ErrorCode::AssertHeightRelativeFailed => "ASSERT_HEIGHT_RELATIVE_FAILED",
        ErrorCode::AssertSecondsRelativeFailed => "ASSERT_SECONDS_RELATIVE_FAILED",
        ErrorCode::AssertBeforeHeightRelativeFailed => "ASSERT_BEFORE_HEIGHT_RELATIVE_FAILED",
        ErrorCode::AssertBeforeSecondsRelativeFailed => "ASSERT_BEFORE_SECONDS_RELATIVE_FAILED",
        _ => "UNKNOWN",
    }
}
