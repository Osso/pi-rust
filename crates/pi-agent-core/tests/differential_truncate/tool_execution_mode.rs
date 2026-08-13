use pi_agent_core::harness::ToolExecutionMode;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const REQUEST_ID: u64 = 13_165;

struct Case {
    name: &'static str,
    mode: ToolExecutionMode,
    source_literal: &'static str,
}

pub(crate) fn compare(oracle: &mut TsOracle) {
    let cases = cases();
    let expected = oracle.request(&request(&cases));
    let actual = rust_response(&cases);
    assert_eq!(actual, expected, "tool execution mode mismatch");
}

fn request(cases: &[Case]) -> Value {
    json!({
        "id": REQUEST_ID,
        "operation": "tool_execution_mode",
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
            name: "sequential",
            mode: ToolExecutionMode::Sequential,
            source_literal: "sequential",
        },
        Case {
            name: "parallel",
            mode: ToolExecutionMode::Parallel,
            source_literal: "parallel",
        },
    ]
}
