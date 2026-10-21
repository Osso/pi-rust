use pi_agent_core::harness::BeforeToolCallResult;
use serde_json::{Map, Value, json};

use super::{oracle::TsOracle, support::ok_response};

const REQUEST_ID: u64 = 13_180;
const EXPECTED_CASE_COUNT: usize = 11;

struct Case {
    name: &'static str,
    block_state: OptionalState,
    block: bool,
    reason_state: OptionalState,
    reason: &'static str,
    terminate_state: OptionalState,
    terminate: bool,
}

#[derive(Clone, Copy)]
enum OptionalState {
    Absent,
    Present,
}

impl OptionalState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Present => "present",
        }
    }
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let cases = cases();
    assert_eq!(cases.len(), EXPECTED_CASE_COUNT);

    let expected = oracle.request(&request(&cases));
    let actual = rust_response(&cases);
    assert_eq!(actual, expected, "before tool call result mismatch");
}

fn request(cases: &[Case]) -> Value {
    json!({
        "id": REQUEST_ID,
        "operation": "before_tool_call_result",
        "cases": cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "blockState": case.block_state.as_str(),
        "block": case.block,
        "reasonState": case.reason_state.as_str(),
        "reason": case.reason,
        "terminateState": case.terminate_state.as_str(),
        "terminate": case.terminate,
    })
}

fn rust_response(cases: &[Case]) -> Value {
    let results = cases.iter().map(normalize_case).collect::<Vec<_>>();
    ok_response(REQUEST_ID, json!({"results": results}))
}

fn normalize_case(case: &Case) -> Value {
    let result = BeforeToolCallResult {
        block: optional_bool(case.block_state, case.block),
        reason: optional_string(case.reason_state, case.reason),
    };
    json!({
        "name": case.name,
        "record": normalize_record(&result),
    })
}

fn optional_bool(state: OptionalState, value: bool) -> Option<bool> {
    match state {
        OptionalState::Absent => None,
        OptionalState::Present => Some(value),
    }
}

fn optional_string(state: OptionalState, value: &str) -> Option<String> {
    match state {
        OptionalState::Absent => None,
        OptionalState::Present => Some(value.to_owned()),
    }
}

fn normalize_record(result: &BeforeToolCallResult) -> Value {
    let mut record = Map::new();
    if let Some(block) = result.block {
        record.insert("block".to_owned(), json!(block));
    }
    if let Some(reason) = &result.reason {
        record.insert("reason".to_owned(), json!(reason));
    }
    Value::Object(record)
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "empty",
            block_state: OptionalState::Absent,
            block: false,
            reason_state: OptionalState::Absent,
            reason: "",
            terminate_state: OptionalState::Absent,
            terminate: false,
        },
        Case {
            name: "block false",
            block_state: OptionalState::Present,
            block: false,
            reason_state: OptionalState::Absent,
            reason: "",
            terminate_state: OptionalState::Absent,
            terminate: false,
        },
        Case {
            name: "block true",
            block_state: OptionalState::Present,
            block: true,
            reason_state: OptionalState::Absent,
            reason: "",
            terminate_state: OptionalState::Absent,
            terminate: false,
        },
        Case {
            name: "empty reason",
            block_state: OptionalState::Absent,
            block: false,
            reason_state: OptionalState::Present,
            reason: "",
            terminate_state: OptionalState::Absent,
            terminate: false,
        },
        Case {
            name: "unicode nul newline reason",
            block_state: OptionalState::Absent,
            block: false,
            reason_state: OptionalState::Present,
            reason: "blocked\n理由\0next\r",
            terminate_state: OptionalState::Absent,
            terminate: false,
        },
        Case {
            name: "terminate false",
            block_state: OptionalState::Absent,
            block: false,
            reason_state: OptionalState::Absent,
            reason: "",
            terminate_state: OptionalState::Present,
            terminate: false,
        },
        Case {
            name: "terminate true",
            block_state: OptionalState::Absent,
            block: false,
            reason_state: OptionalState::Absent,
            reason: "",
            terminate_state: OptionalState::Present,
            terminate: true,
        },
        Case {
            name: "block and reason",
            block_state: OptionalState::Present,
            block: true,
            reason_state: OptionalState::Present,
            reason: "blocked",
            terminate_state: OptionalState::Absent,
            terminate: false,
        },
        Case {
            name: "block and terminate",
            block_state: OptionalState::Present,
            block: false,
            reason_state: OptionalState::Absent,
            reason: "",
            terminate_state: OptionalState::Present,
            terminate: true,
        },
        Case {
            name: "reason and terminate",
            block_state: OptionalState::Absent,
            block: false,
            reason_state: OptionalState::Present,
            reason: "review required",
            terminate_state: OptionalState::Present,
            terminate: false,
        },
        Case {
            name: "fully populated",
            block_state: OptionalState::Present,
            block: true,
            reason_state: OptionalState::Present,
            reason: "blocked\n理由\0next\r",
            terminate_state: OptionalState::Present,
            terminate: true,
        },
    ]
}
