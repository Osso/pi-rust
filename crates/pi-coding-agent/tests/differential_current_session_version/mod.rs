use crate::shared_oracle as oracle;

use pi_coding_agent::CURRENT_SESSION_VERSION;
use pi_coding_agent::core::session_manager::CURRENT_SESSION_VERSION as MODULE_CURRENT_SESSION_VERSION;
use serde_json::json;

const REQUEST_ID: u64 = 13_179;

pub(crate) fn compare() {
    assert_eq!(
        CURRENT_SESSION_VERSION.to_bits(),
        MODULE_CURRENT_SESSION_VERSION.to_bits()
    );
    assert_eq!(CURRENT_SESSION_VERSION.to_bits(), 3.0_f64.to_bits());

    let mut oracle = oracle::TsOracle::spawn(
        "current-session-version-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let request = json!({
        "id": REQUEST_ID,
        "operation": "current_session_version",
    });
    let expected = oracle.request(&request);
    let actual = json!({
        "id": REQUEST_ID,
        "ok": true,
        "output": {"value": 3},
    });
    assert_eq!(actual, expected, "current session version mismatch");
}
