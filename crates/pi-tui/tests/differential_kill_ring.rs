use crate::shared_oracle as oracle;

use pi_tui::kill_ring::KillRing;
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

#[derive(Clone, Copy)]
enum Operation {
    Push {
        text: &'static str,
        prepend: bool,
        accumulate: bool,
    },
    Rotate,
    Peek,
}

const CASES: &[&[Operation]] = &[
    &[
        Operation::Push {
            text: "",
            prepend: false,
            accumulate: false,
        },
        Operation::Peek,
    ],
    &[
        Operation::Push {
            text: "first",
            prepend: false,
            accumulate: false,
        },
        Operation::Push {
            text: "second",
            prepend: false,
            accumulate: false,
        },
        Operation::Peek,
    ],
    &[
        Operation::Push {
            text: "abc",
            prepend: false,
            accumulate: false,
        },
        Operation::Push {
            text: "def",
            prepend: false,
            accumulate: true,
        },
        Operation::Peek,
    ],
    &[
        Operation::Push {
            text: "def",
            prepend: false,
            accumulate: false,
        },
        Operation::Push {
            text: "abc",
            prepend: true,
            accumulate: true,
        },
        Operation::Peek,
    ],
    &[
        Operation::Push {
            text: "only",
            prepend: true,
            accumulate: true,
        },
        Operation::Peek,
    ],
    &[
        Operation::Push {
            text: "one",
            prepend: false,
            accumulate: false,
        },
        Operation::Push {
            text: "two",
            prepend: false,
            accumulate: false,
        },
        Operation::Push {
            text: "three",
            prepend: false,
            accumulate: false,
        },
        Operation::Rotate,
        Operation::Peek,
        Operation::Rotate,
        Operation::Peek,
        Operation::Rotate,
        Operation::Peek,
    ],
    &[Operation::Rotate, Operation::Peek],
];

fn simulate(operations: &[Operation]) -> Value {
    let mut ring = KillRing::new();
    let mut snapshots = Vec::new();
    for operation in operations {
        match *operation {
            Operation::Push {
                text,
                prepend,
                accumulate,
            } => ring.push(&text.into(), prepend, accumulate),
            Operation::Rotate => ring.rotate(),
            Operation::Peek => snapshots.push(json!({
                "peek": ring.peek(),
                "length": ring.len(),
            })),
        }
    }

    let mut entries = Vec::new();
    let length = ring.len();
    for _ in 0..length {
        entries.push(json!(ring.peek()));
        ring.rotate();
    }
    json!({ "snapshots": snapshots, "length": ring.len(), "entries": entries })
}

fn oracle_operations(operations: &[Operation]) -> Vec<Value> {
    operations
        .iter()
        .map(|operation| match *operation {
            Operation::Push {
                text,
                prepend,
                accumulate,
            } => json!({
                "kind": "push",
                "text": text,
                "prepend": prepend,
                "accumulate": accumulate,
            }),
            Operation::Rotate => json!({ "kind": "rotate" }),
            Operation::Peek => json!({ "kind": "peek" }),
        })
        .collect()
}

#[test]
fn kill_ring_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let cases: Vec<Value> = CASES
        .iter()
        .map(|operations| json!(oracle_operations(operations)))
        .collect();
    let response = oracle.request(&json!({
        "id": "kill-ring",
        "operation": "kill_ring",
        "cases": cases,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let results = response["output"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), CASES.len());
    for (operations, expected) in CASES.iter().zip(results) {
        assert_eq!(&simulate(operations), expected);
    }
}
