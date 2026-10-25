use crate::shared_oracle as oracle;

use pi_agent_core::uuidv7;
use serde_json::json;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-uuid-oracle.ts",
        "packages/agent-core/src/harness/session/uuid.ts",
    );
    let response = oracle.request(&json!({"id": 1, "operation": "generate", "count": 200}));
    assert_eq!(response["ok"], json!(true), "uuid oracle failed: {response}");

    let before = response["output"]["before"]
        .as_i64()
        .expect("before must be an integer");
    let after = response["output"]["after"].as_i64().expect("after must be an integer");
    let values = response["output"]["values"]
        .as_array()
        .expect("values must be an array");
    assert_eq!(values.len(), 200);

    let mut previous: Option<&str> = None;
    for value in values {
        let uuid = value.as_str().expect("uuid must be a string");
        assert_layout(uuid);
        let timestamp = timestamp_of(uuid);
        assert!(
            timestamp >= before && timestamp <= after,
            "{uuid} timestamp outside [{before}, {after}]"
        );
        if let Some(previous) = previous {
            assert!(previous < uuid, "{previous} must sort before {uuid}");
        }
        previous = Some(uuid);
    }

    let rust_uuid = uuidv7();
    assert_layout(&rust_uuid);
}

fn assert_layout(uuid: &str) {
    assert_eq!(uuid.len(), 36, "{uuid}");
    let bytes = uuid.as_bytes();
    for index in [8, 13, 18, 23] {
        assert_eq!(bytes[index], b'-', "{uuid}");
    }
    assert!(
        uuid.chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-'),
        "{uuid}"
    );
    assert_eq!(&uuid[14..15], "7", "{uuid}");
    assert!(matches!(&uuid[19..20], "8" | "9" | "a" | "b"), "{uuid}");
}

fn timestamp_of(uuid: &str) -> i64 {
    i64::from_str_radix(&uuid.replace('-', "")[..12], 16).expect("timestamp must be hex")
}
