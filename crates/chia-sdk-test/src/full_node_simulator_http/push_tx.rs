use chia_protocol::Bytes32;
use chia_sdk_coinset::PushTxResponse;

pub(super) fn push_tx_response_body(
    spend_name: Bytes32,
    response: PushTxResponse,
) -> serde_json::Value {
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
