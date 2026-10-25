use crate::shared_oracle as oracle;

use pi_coding_agent::core::session_tool_output::{
    MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES as MODULE_MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES,
    TOOL_RESULT_TRUNCATION_MARKER as MODULE_TOOL_RESULT_TRUNCATION_MARKER,
};
use pi_coding_agent::{MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES, TOOL_RESULT_TRUNCATION_MARKER};
use serde_json::json;

const REQUEST_ID: u64 = 13_178;

pub(crate) fn compare() {
    assert_eq!(
        MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES,
        MODULE_MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES
    );
    assert_eq!(TOOL_RESULT_TRUNCATION_MARKER, MODULE_TOOL_RESULT_TRUNCATION_MARKER);

    let mut oracle = oracle::TsOracle::spawn(
        "tool-output-persistence-constants-oracle.ts",
        "packages/coding-agent/src/core/session-tool-output.ts",
    );
    let request = json!({
        "id": REQUEST_ID,
        "operation": "tool_output_persistence_constants",
    });
    let expected = oracle.request(&request);
    let actual = json!({
        "id": REQUEST_ID,
        "ok": true,
        "output": {
            "maxPersistedToolResultContentBytes": MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES,
            "toolResultTruncationMarker": TOOL_RESULT_TRUNCATION_MARKER,
        },
    });
    assert_eq!(actual, expected, "tool-output persistence constants mismatch");
}
