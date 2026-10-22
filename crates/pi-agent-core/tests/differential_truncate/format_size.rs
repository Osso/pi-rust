use pi_agent_core::harness::utils::format_size;
use serde_json::json;

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 10_000;

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = [
        ("zero", 0),
        ("last byte", 1_023),
        ("one KiB", 1_024),
        ("half-up KiB", 1_280),
        ("last KiB branch", 1_048_575),
        ("one MiB", 1_048_576),
        ("one and a half MiB", 1_572_864),
        ("half-up MiB", 1_310_720),
    ];

    for (offset, (name, bytes)) in fixtures.into_iter().enumerate() {
        let id = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        let expected = oracle.request(&json!({"id": id, "operation": "format_size", "bytes": bytes}));
        let actual = ok_response(id, json!({"value": format_size(bytes as f64)}));

        assert_eq!(actual, expected, "format-size mismatch: {name}");
    }
}
