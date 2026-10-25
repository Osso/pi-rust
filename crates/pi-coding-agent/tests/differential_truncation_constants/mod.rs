use crate::shared_oracle as oracle;

use pi_coding_agent::core::tools::truncate::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH};
use serde_json::json;

const REQUEST_ID: u64 = 13_186;

pub(crate) fn compare() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-truncation-constants-oracle.ts",
        "packages/coding-agent/src/core/tools/truncate.ts",
    );
    let expected = oracle.request(&json!({
        "id": REQUEST_ID,
        "operation": "coding_agent_truncation_constants",
    }));
    let actual = json!({
        "id": REQUEST_ID,
        "ok": true,
        "output": {
            "defaultMaxLines": DEFAULT_MAX_LINES,
            "defaultMaxBytes": DEFAULT_MAX_BYTES,
            "grepMaxLineLength": GREP_MAX_LINE_LENGTH,
        },
    });

    assert_eq!(actual, expected, "coding-agent truncation constants mismatch");
}
