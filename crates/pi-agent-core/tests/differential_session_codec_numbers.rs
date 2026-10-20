#[path = "../../pi-coding-agent/tests/support/oracle.rs"]
mod oracle;

use pi_agent_core::harness::session::SessionMutation;
use pi_agent_core::harness::session::jsonl::codec::{parse_header, parse_mutation};
use pi_agent_core::harness::session::types::JsonlDecodeError;
use serde::Serialize;
use serde_json::{Value, json};

fn decode_result<T: Serialize>(result: Result<T, JsonlDecodeError>) -> Value {
    match result {
        Ok(value) => json!({"ok": true, "value": value}),
        Err(error) => json!({"ok": false, "error": {"kind": error.kind.as_str(), "message": error.message}}),
    }
}

fn compare(oracle: &mut oracle::TsOracle, operation: &str, line: &str, error: Option<&str>) {
    let request = json!({"operation": operation, "line": line});
    let expected = oracle.request(&request);
    if let Some(message) = error {
        assert_eq!(
            expected,
            json!({"ok": false, "error": {"kind": "schema", "message": message}}),
            "{request}"
        );
    } else {
        assert_eq!(expected["ok"], true, "{request}: {expected}");
    }
    let actual = if operation == "header" {
        decode_result(parse_header(line))
    } else {
        decode_result(parse_mutation(line).map(mutation_value))
    };
    assert_eq!(actual, expected, "{request}");
}

// SessionMutation's serde representation is the on-disk flattened format, not
// the parseMutation API's entry/record wrapper. Represent the complete API value.
fn mutation_value(mutation: SessionMutation) -> Value {
    match mutation {
        SessionMutation::Entry { lane, entry } => {
            let mut value = json!({"kind": "entry", "entry": entry});
            if let Some(lane) = lane {
                value["lane"] = json!(lane);
            }
            value
        }
        SessionMutation::Record { record } => json!({"kind": "record", "record": record}),
        other => serde_json::to_value(other).expect("mutation must serialize"),
    }
}

fn spawn_oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "session-codec-numbers-oracle.ts",
        "packages/agent/src/harness/session/jsonl/codec.ts",
    )
}

#[test]
fn numeric_boundaries_match_pinned_codec_without_result_normalization() {
    let mut oracle = spawn_oracle();
    // Keep raw JSON spelling: serializing a Rust number would erase 1.0 / exponent cases.
    for (number, sequence_valid, timestamp_valid) in [
        ("9007199254740991", true, true),
        ("9007199254740992", false, false),
        ("-9007199254740991", false, false),
        ("-9007199254740992", false, false),
        ("1", true, true),
        ("0", false, true),
        ("-1", false, false),
        ("-0.0", false, true),
        ("1.0", true, true),
        ("0.0", false, true),
        ("1.5", false, false),
        ("1e0", true, true),
        ("10e-1", true, true),
        ("1e-1", false, false),
        ("9.007199254740991e15", true, true),
        ("9.007199254740992e15", false, false),
    ] {
        let header = format!(r#"{{"kind":"header","version":4,"id":"s","createdAt":{number},"cwd":"/project"}}"#);
        compare(
            &mut oracle,
            "header",
            &header,
            (!timestamp_valid).then_some("has invalid timestamp"),
        );
        for template in [
            r#"{"kind":"entry","seq":SEQ,"id":"e","type":"custom","parentId":null,"timestamp":TIME,"customType":"test"}"#,
            r#"{"kind":"record","seq":SEQ,"id":"r","lane":"main","type":"abort_requested","runId":"run","timestamp":TIME}"#,
            r#"{"kind":"lane","seq":SEQ,"lane":"main","leafId":null}"#,
            r#"{"kind":"fact","seq":SEQ,"fact":"name","name":"test"}"#,
        ] {
            let line = template.replace("SEQ", number).replace("TIME", "0");
            compare(
                &mut oracle,
                "mutation",
                &line,
                (!sequence_valid).then_some("has invalid seq"),
            );
            if template.contains("TIME") {
                let line = template.replace("SEQ", "1").replace("TIME", number);
                compare(
                    &mut oracle,
                    "mutation",
                    &line,
                    (!timestamp_valid).then_some("has invalid timestamp"),
                );
            }
        }
    }
}

#[test]
fn numeric_errors_follow_pinned_codec_validation_order() {
    let mut oracle = spawn_oracle();
    for (operation, line, message) in [
        (
            "mutation",
            r#"{"kind":"unknown","seq":9007199254740992,"timestamp":-1}"#,
            "has invalid seq",
        ),
        (
            "mutation",
            r#"{"kind":"entry","seq":9007199254740992,"id":null,"timestamp":-1}"#,
            "has invalid seq",
        ),
        (
            "mutation",
            r#"{"kind":"entry","seq":1,"id":null,"timestamp":9007199254740992}"#,
            "has invalid id",
        ),
        (
            "mutation",
            r#"{"kind":"entry","seq":1,"id":"e","type":"custom","parentId":false,"timestamp":9007199254740992}"#,
            "has invalid parentId",
        ),
        (
            "mutation",
            r#"{"kind":"entry","seq":1,"id":"e","type":"custom","parentId":null,"timestamp":9007199254740992}"#,
            "has invalid timestamp",
        ),
        (
            "mutation",
            r#"{"kind":"record","seq":1,"id":"r","lane":null,"timestamp":9007199254740992}"#,
            "has invalid lane",
        ),
        (
            "mutation",
            r#"{"kind":"record","seq":1,"id":"r","lane":"main","type":"operation_started","timestamp":9007199254740992,"intent":null}"#,
            "has invalid timestamp",
        ),
        (
            "header",
            r#"{"kind":"other","version":3,"createdAt":9007199254740992}"#,
            "is not a header",
        ),
        (
            "header",
            r#"{"kind":"header","version":3,"createdAt":9007199254740992}"#,
            "has unsupported session version",
        ),
        (
            "header",
            r#"{"kind":"header","version":4,"id":"s","parentSessionId":false,"createdAt":9007199254740992}"#,
            "has invalid parentSessionId",
        ),
        (
            "header",
            r#"{"kind":"header","version":4,"id":null,"createdAt":9007199254740992}"#,
            "has invalid id",
        ),
        (
            "header",
            r#"{"kind":"header","version":4,"id":"s","cwd":null,"createdAt":9007199254740992}"#,
            "has invalid timestamp",
        ),
    ] {
        compare(&mut oracle, operation, line, Some(message));
    }
}

#[test]
fn header_version_uses_javascript_numeric_equality() {
    let mut oracle = spawn_oracle();
    for (version, valid) in [
        ("4", true),
        ("4.0", true),
        ("4e0", true),
        ("4.5", false),
        ("3", false),
        (r#""4""#, false),
    ] {
        let line = format!(r#"{{"kind":"header","version":{version},"id":"s","createdAt":0,"cwd":"/project"}}"#);
        compare(
            &mut oracle,
            "header",
            &line,
            (!valid).then_some("has unsupported session version"),
        );
    }
}
