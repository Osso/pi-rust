//! Node's scheduled delay, observed without waiting for oversized timers.
use crate::shared_oracle_2 as oracle;

use pi_js_compat::node_timers::normalize_timeout_delay;
use pi_js_compat::number::parse_number;
use serde_json::json;

#[test]
fn timeout_delays_match_pinned_node_runtime() {
    let values = [
        "NaN",
        "Infinity",
        "-Infinity",
        "-1",
        "-0",
        "0",
        "0.9",
        "1",
        "1.9",
        "2.5",
        "2147483646.9",
        "2147483647",
        "2147483647.1",
        "2147483648",
        "1e103",
    ];
    let mut oracle = oracle::TsOracle::spawn("node-timers-oracle.ts", "packages/coding-agent/src/index.ts");
    let response = oracle.request(&json!(values));
    let expected: Vec<f64> = response
        .as_array()
        .unwrap()
        .iter()
        .map(|delay| delay.as_f64().unwrap())
        .collect();
    let actual: Vec<f64> = values
        .iter()
        .map(|value| normalize_timeout_delay(parse_number(value)))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(normalize_timeout_delay(1.9), 1.9);
    assert_eq!(normalize_timeout_delay(2_147_483_647.0), 2_147_483_647.0);
}
