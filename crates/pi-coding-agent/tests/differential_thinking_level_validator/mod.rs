use crate::shared_oracle as oracle;

use pi_coding_agent::cli::args::is_valid_thinking_level;
use serde_json::json;

const REQUEST_ID: u64 = 13_177;
const LEVELS: [&str; 19] = [
    "off",
    "minimal",
    "low",
    "medium",
    "high",
    "xhigh",
    "max",
    "ultra",
    "",
    "OFF",
    "Medium",
    " medium",
    "medium ",
    "x-high",
    "extreme",
    "unsupported",
    "é😀",
    "\0",
    "\n",
];

pub(crate) fn compare() {
    let mut oracle = oracle::TsOracle::spawn(
        "thinking-level-validator-oracle.ts",
        "packages/coding-agent/src/cli/args.ts",
    );
    let request = json!({
        "id": REQUEST_ID,
        "operation": "thinking_level_validator",
        "levels": LEVELS,
    });
    let expected = oracle.request(&request);
    let actual = json!({
        "id": REQUEST_ID,
        "ok": true,
        "output": {
            "results": LEVELS.iter().map(|value| json!({
                "value": value,
                "valid": is_valid_thinking_level(value),
            })).collect::<Vec<_>>(),
        },
    });
    assert_eq!(actual, expected, "thinking-level validator mismatch");
}
