use crate::shared_oracle as oracle;

use pi_coding_agent::core::defaults::DEFAULT_THINKING_LEVEL;
use serde_json::json;

const REQUEST_ID: u64 = 13_167;

pub(crate) fn compare() {
    let mut oracle = oracle::TsOracle::spawn(
        "default-thinking-level-oracle.ts",
        "packages/coding-agent/src/core/defaults.ts",
    );
    let request = json!({
        "id": REQUEST_ID,
        "operation": "default_thinking_level",
    });
    let expected = oracle.request(&request);
    let actual = json!({
        "id": REQUEST_ID,
        "ok": true,
        "output": {"value": DEFAULT_THINKING_LEVEL.as_str()},
    });
    assert_eq!(actual, expected, "default thinking level mismatch");
}
