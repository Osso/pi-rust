use crate::shared_oracle as oracle;

use pi_coding_agent::cli::args::Mode;
use serde_json::json;

const REQUEST_ID: u64 = 13_176;
const MODES: [Mode; 3] = [Mode::Text, Mode::Json, Mode::Rpc];

pub(crate) fn compare() {
    let mut oracle = oracle::TsOracle::spawn("cli-mode-oracle.ts", "packages/coding-agent/src/cli/args.ts");
    let request = json!({
        "id": REQUEST_ID,
        "operation": "cli_mode",
        "modes": ["text", "json", "rpc"],
    });
    let expected = oracle.request(&request);
    let actual = json!({
        "id": REQUEST_ID,
        "ok": true,
        "output": {
            "values": MODES.iter().map(|mode| mode.as_str()).collect::<Vec<_>>(),
        },
    });
    assert_eq!(actual, expected, "CLI mode mismatch");
}
