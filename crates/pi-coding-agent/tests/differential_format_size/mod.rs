use crate::shared_oracle as oracle;

use pi_coding_agent::format_size;
use serde_json::{Value, json};

const REQUEST_ID: u64 = 13_185;
const EXPECTED_CASE_COUNT: usize = 16;

struct Case {
    name: &'static str,
    bytes: usize,
}

const CASES: [Case; EXPECTED_CASE_COUNT] = [
    Case {
        name: "zero bytes",
        bytes: 0,
    },
    Case {
        name: "one byte",
        bytes: 1,
    },
    Case {
        name: "last byte branch value",
        bytes: 1_023,
    },
    Case {
        name: "one kibibyte",
        bytes: 1_024,
    },
    Case {
        name: "one byte above kibibyte",
        bytes: 1_025,
    },
    Case {
        name: "below half-up kibibyte",
        bytes: 1_279,
    },
    Case {
        name: "half-up kibibyte",
        bytes: 1_280,
    },
    Case {
        name: "above half-up kibibyte",
        bytes: 1_281,
    },
    Case {
        name: "five hundred twelve kibibytes",
        bytes: 524_288,
    },
    Case {
        name: "1023.5 kibibytes",
        bytes: 1_048_064,
    },
    Case {
        name: "last kibibyte branch value",
        bytes: 1_048_575,
    },
    Case {
        name: "one mebibyte",
        bytes: 1_048_576,
    },
    Case {
        name: "one byte above mebibyte",
        bytes: 1_048_577,
    },
    Case {
        name: "one and a quarter mebibytes",
        bytes: 1_310_720,
    },
    Case {
        name: "one and a half mebibytes",
        bytes: 1_572_864,
    },
    Case {
        name: "one gibibyte",
        bytes: 1_073_741_824,
    },
];

pub(crate) fn compare() {
    assert_eq!(CASES.len(), EXPECTED_CASE_COUNT);

    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-format-size-oracle.ts",
        "packages/coding-agent/src/core/tools/truncate.ts",
    );
    let expected = oracle.request(&request(&CASES));
    let actual = response(&CASES);

    assert_eq!(actual, expected, "coding-agent format-size mismatch");
}

fn request(cases: &[Case]) -> Value {
    json!({
        "id": REQUEST_ID,
        "operation": "coding_agent_format_size",
        "cases": cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({"name": case.name, "bytes": case.bytes})
}

fn response(cases: &[Case]) -> Value {
    let results = cases
        .iter()
        .map(|case| json!({"name": case.name, "value": format_size(case.bytes as f64)}))
        .collect::<Vec<_>>();
    json!({
        "id": REQUEST_ID,
        "ok": true,
        "output": {"results": results},
    })
}
