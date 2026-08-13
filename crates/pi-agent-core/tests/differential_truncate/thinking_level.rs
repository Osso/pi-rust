use pi_agent_core::harness::ThinkingLevel;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const REQUEST_ID: u64 = 13_166;

struct Case {
    name: &'static str,
    level: ThinkingLevel,
    source_literal: &'static str,
}

const CASES: [Case; 8] = [
    Case {
        name: "off",
        level: ThinkingLevel::Off,
        source_literal: "off",
    },
    Case {
        name: "minimal",
        level: ThinkingLevel::Minimal,
        source_literal: "minimal",
    },
    Case {
        name: "low",
        level: ThinkingLevel::Low,
        source_literal: "low",
    },
    Case {
        name: "medium",
        level: ThinkingLevel::Medium,
        source_literal: "medium",
    },
    Case {
        name: "high",
        level: ThinkingLevel::High,
        source_literal: "high",
    },
    Case {
        name: "xhigh",
        level: ThinkingLevel::XHigh,
        source_literal: "xhigh",
    },
    Case {
        name: "max",
        level: ThinkingLevel::Max,
        source_literal: "max",
    },
    Case {
        name: "ultra",
        level: ThinkingLevel::Ultra,
        source_literal: "ultra",
    },
];

pub(crate) fn compare(oracle: &mut TsOracle) {
    let expected = oracle.request(&request(&CASES));
    let actual = rust_response(&CASES);
    assert_eq!(actual, expected, "thinking level mismatch");
}

fn request(cases: &[Case]) -> Value {
    json!({
        "id": REQUEST_ID,
        "operation": "thinking_level",
        "cases": cases.iter().map(request_case).collect::<Vec<_>>(),
    })
}

fn request_case(case: &Case) -> Value {
    json!({
        "name": case.name,
        "level": case.source_literal,
    })
}

fn rust_response(cases: &[Case]) -> Value {
    let results = cases.iter().map(rust_case).collect::<Vec<_>>();
    ok_response(REQUEST_ID, json!({"results": results}))
}

fn rust_case(case: &Case) -> Value {
    json!({
        "name": case.name,
        "level": case.level.as_str(),
    })
}
