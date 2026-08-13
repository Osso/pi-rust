use pi_agent_core::harness::QueueMode;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const REQUEST_ID: u64 = 13_164;

struct Case {
    name: &'static str,
    mode: QueueMode,
    source_literal: &'static str,
}

pub(crate) fn compare(oracle: &mut TsOracle) {
    let cases = cases();
    let expected = oracle.request(&request(&cases));
    let actual = rust_response(&cases);
    assert_eq!(actual, expected, "queue mode mismatch");
}

fn request(cases: &[Case]) -> Value {
    json!({
        "id": REQUEST_ID,
        "operation": "queue_mode",
        "cases": cases.iter().map(request_case).collect::<Vec<_>>(),
    })
}

fn request_case(case: &Case) -> Value {
    json!({
        "name": case.name,
        "mode": case.source_literal,
    })
}

fn rust_response(cases: &[Case]) -> Value {
    let results = cases.iter().map(rust_case).collect::<Vec<_>>();
    ok_response(REQUEST_ID, json!({"results": results}))
}

fn rust_case(case: &Case) -> Value {
    json!({
        "name": case.name,
        "mode": case.mode.as_str(),
    })
}

fn cases() -> [Case; 2] {
    [
        Case {
            name: "all",
            mode: QueueMode::All,
            source_literal: "all",
        },
        Case {
            name: "one at a time",
            mode: QueueMode::OneAtATime,
            source_literal: "one-at-a-time",
        },
    ]
}
