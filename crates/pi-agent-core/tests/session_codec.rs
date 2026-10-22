use pi_agent_core::harness::session::jsonl::codec::{
    encode_header, encode_mutation, metadata_from_header, parse_header, parse_mutation,
};
use pi_agent_core::harness::session::types::{JsonlDecodeKind, SessionMutation};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

fn mutation(value: Value) -> SessionMutation {
    parse_mutation(&value.to_string()).expect("mutation fixture must parse")
}

fn encoded(value: Value) -> String {
    encode_mutation(&mutation(value)).expect("mutation fixture must encode")
}

#[test]
fn header_round_trips_every_field_with_a_resolved_parent() {
    let header = json!({
        "kind": "header",
        "version": 4,
        "id": "session",
        "createdAt": 1700000000000_i64,
        "cwd": "/workspace/project",
        "parentSessionId": "parent",
        "metadata": {"owner": "agent", "nested": {"enabled": true}, "values": [1, null, "two"]},
    });
    let encoded = encode_header(&parse_header(&header.to_string()).unwrap());
    assert!(encoded.ends_with('\n'));
    assert_eq!(encoded, format!("{header}\n"));
    assert_eq!(
        parse_header(encoded.trim_end()).unwrap(),
        parse_header(&header.to_string()).unwrap()
    );
}

#[test]
fn header_round_trips_an_unresolved_legacy_parent_path() {
    let header = json!({
        "kind": "header",
        "version": 4,
        "id": "legacy-child",
        "createdAt": 1700000000001_i64,
        "cwd": "/workspace/project",
        "legacyParentSessionPath": "/sessions/missing-parent.jsonl",
    });
    let parsed = parse_header(&header.to_string()).unwrap();
    assert_eq!(
        parsed.legacy_parent_session_path.as_ref(),
        Some(&JsString::from("/sessions/missing-parent.jsonl"))
    );
    assert_eq!(encode_header(&parsed), format!("{header}\n"));
}

#[test]
fn header_projects_filesystem_fields_into_metadata() {
    let header = json!({
        "kind": "header",
        "version": 4,
        "id": "session",
        "createdAt": 1700000000000_i64,
        "cwd": "/workspace/project",
        "legacyParentSessionPath": "/sessions/missing-parent.jsonl",
        "metadata": {"owner": "agent"},
    });
    let parsed = parse_header(&header.to_string()).unwrap();
    let metadata = metadata_from_header(&parsed, "/sessions/session.jsonl".into(), 1_700_000_000_100.0);
    assert_eq!(metadata.id, "session");
    assert_eq!(metadata.created_at, 1_700_000_000_000);
    assert_eq!(metadata.cwd, "/workspace/project");
    assert_eq!(metadata.path, "/sessions/session.jsonl");
    assert_eq!(metadata.modified_at, 1_700_000_000_100.0);
    assert_eq!(metadata.source_format, 4);
    assert_eq!(
        metadata.legacy_parent_session_path.as_ref(),
        Some(&JsString::from("/sessions/missing-parent.jsonl"))
    );
    assert_eq!(metadata.metadata.as_ref().unwrap()["owner"], json!("agent"));
    assert_eq!(
        serde_json::to_string(&metadata).unwrap(),
        r#"{"id":"session","createdAt":1700000000000,"cwd":"/workspace/project","path":"/sessions/session.jsonl","modifiedAt":1700000000100,"sourceFormat":4,"legacyParentSessionPath":"/sessions/missing-parent.jsonl","metadata":{"owner":"agent"}}"#
    );
}

#[test]
fn rejects_syntax_and_unknown_mutation_kinds() {
    let syntax = parse_mutation("{").unwrap_err();
    assert_eq!(syntax.kind, JsonlDecodeKind::Syntax);
    assert_eq!(syntax.message, "is not valid JSON");

    let schema = parse_mutation(&json!({"kind": "unknown", "seq": 1}).to_string()).unwrap_err();
    assert_eq!(schema.kind, JsonlDecodeKind::Schema);
    assert_eq!(schema.message, "has unknown mutation kind");
}

#[test]
fn round_trips_a_lane_bound_entry_line_byte_for_byte() {
    let line = r#"{"kind":"entry","lane":"main","type":"custom","id":"entry-1","customType":"note","data":{"text":"hello"},"parentId":null,"seq":1,"timestamp":100}"#;
    assert_eq!(encoded(serde_json::from_str(line).unwrap()), format!("{line}\n"));
}

#[test]
fn round_trips_an_imported_entry_line_without_a_lane() {
    let line = r#"{"kind":"entry","type":"custom","id":"entry-1","customType":"note","parentId":null,"seq":1,"timestamp":100}"#;
    assert_eq!(encoded(serde_json::from_str(line).unwrap()), format!("{line}\n"));
}

#[test]
fn round_trips_a_record_line_byte_for_byte() {
    let line = r#"{"kind":"record","type":"operation_started","id":"run-1","lane":"main","sourceLeafId":null,"intent":{"kind":"run","originalPrompt":[],"initialMessages":[]},"seq":1,"timestamp":100}"#;
    assert_eq!(encoded(serde_json::from_str(line).unwrap()), format!("{line}\n"));
}

#[test]
fn round_trips_lane_and_fact_lines_including_cleared_values() {
    let lane = r#"{"kind":"lane","seq":1,"lane":"thread","leafId":"entry-1"}"#;
    assert_eq!(encoded(serde_json::from_str(lane).unwrap()), format!("{lane}\n"));

    let name = r#"{"kind":"fact","seq":1,"fact":"name","name":"Example"}"#;
    assert_eq!(encoded(serde_json::from_str(name).unwrap()), format!("{name}\n"));

    let cleared = r#"{"kind":"fact","seq":2,"fact":"name"}"#;
    assert_eq!(encoded(serde_json::from_str(cleared).unwrap()), format!("{cleared}\n"));

    let label = r#"{"kind":"fact","seq":3,"fact":"label","targetId":"entry-1","label":"checkpoint"}"#;
    assert_eq!(encoded(serde_json::from_str(label).unwrap()), format!("{label}\n"));
}

#[test]
fn rejects_incomplete_entry_and_record_payloads() {
    let cases = [
        json!({"kind": "entry", "type": "custom", "id": "entry", "parentId": null, "seq": 1, "timestamp": 1}),
        json!({
            "kind": "record",
            "type": "operation_started",
            "id": "run",
            "lane": "main",
            "seq": 1,
            "timestamp": 1,
            "sourceLeafId": null,
        }),
        json!({
            "kind": "record",
            "type": "operation_finished",
            "id": "finish",
            "lane": "main",
            "seq": 1,
            "timestamp": 1,
            "outcome": "completed",
        }),
    ];
    for case in cases {
        let error = parse_mutation(&case.to_string()).unwrap_err();
        assert_eq!(error.kind, JsonlDecodeKind::Schema, "case {case}");
    }
}

#[test]
fn rejects_unknown_entry_and_record_types() {
    let entry = parse_mutation(
        &json!({
            "kind": "entry",
            "type": "unknown",
            "id": "entry",
            "parentId": null,
            "seq": 1,
            "timestamp": 1,
        })
        .to_string(),
    )
    .unwrap_err();
    assert_eq!(entry.message, "has unknown entry type unknown");

    let record = parse_mutation(
        &json!({
            "kind": "record",
            "type": "unknown",
            "id": "record",
            "lane": "main",
            "seq": 1,
            "timestamp": 1,
        })
        .to_string(),
    )
    .unwrap_err();
    assert_eq!(record.message, "has unknown record type unknown");
}

#[test]
fn encodes_every_entry_variant_in_typescript_key_order() {
    let cases = [
        r#"{"kind":"entry","lane":"main","type":"message","id":"m","message":{"role":"user","content":[{"type":"text","text":"hi"}],"timestamp":1},"terminate":true,"parentId":null,"seq":1,"timestamp":2}"#,
        r#"{"kind":"entry","lane":"main","type":"model_change","id":"m","provider":"openai","modelId":"gpt-5","parentId":null,"seq":1,"timestamp":2}"#,
        r#"{"kind":"entry","lane":"main","type":"thinking_level_change","id":"m","thinkingLevel":"high","parentId":null,"seq":1,"timestamp":2}"#,
        r#"{"kind":"entry","lane":"main","type":"active_tools_change","id":"m","activeToolNames":["read","bash"],"parentId":null,"seq":1,"timestamp":2}"#,
        r#"{"kind":"entry","lane":"main","type":"compaction","id":"m","summary":"s","tokensBefore":100,"retainedTail":[],"durationMs":5,"details":{"a":1},"parentId":null,"seq":1,"timestamp":2}"#,
        r#"{"kind":"entry","lane":"main","type":"branch_summary","id":"m","fromId":"old","summary":"s","details":{"a":1},"parentId":null,"seq":1,"timestamp":2}"#,
        r#"{"kind":"entry","lane":"main","type":"custom","id":"m","customType":"note","data":null,"parentId":null,"seq":1,"timestamp":2}"#,
    ];
    for line in cases {
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(encoded(value), format!("{line}\n"));
    }
}

#[test]
fn encodes_every_record_variant_in_typescript_key_order() {
    let cases = [
        r#"{"kind":"record","type":"abort_requested","id":"r","lane":"main","runId":"run","seq":1,"timestamp":2}"#,
        r#"{"kind":"record","type":"operation_finished","id":"r","lane":"main","runId":"run","outcome":"failed","error":{"code":"x","message":"y"},"seq":1,"timestamp":2}"#,
        r#"{"kind":"record","type":"step_attempt","id":"r","lane":"main","runId":"run","step":"compaction","attempt":2,"resultEntryId":"entry","compactionReason":"manual","seq":1,"timestamp":2}"#,
        r#"{"kind":"record","type":"tool_started","id":"r","lane":"main","runId":"run","assistantEntryId":"assistant","toolIndex":0,"toolCallId":"call","toolName":"read","effectiveArgs":{"path":"README.md"},"resultEntryId":"result","replay":"safe","seq":1,"timestamp":2}"#,
        r#"{"kind":"record","type":"queue_enqueued","id":"r","lane":"main","queue":"nextRun","target":{"type":"message","id":"queued","message":{"role":"user","content":[{"type":"text","text":"hi"}],"timestamp":1}},"seq":1,"timestamp":2}"#,
        r#"{"kind":"record","type":"queue_cancelled","id":"r","lane":"main","runId":"run","entryId":"entry","seq":1,"timestamp":2}"#,
        r#"{"kind":"record","type":"write_deferred","id":"r","lane":"main","runId":"run","target":{"type":"custom","id":"deferred","customType":"fact"},"seq":1,"timestamp":2}"#,
        r#"{"kind":"record","type":"usage","id":"r","lane":"main","cause":"assistant","runId":"run","entryId":"entry","attempt":1,"stopReason":"stop","usage":{"input":1,"output":2,"cacheRead":3,"cacheWrite":4,"totalTokens":10,"cost":{"input":0.1,"output":0.2,"cacheRead":0.3,"cacheWrite":0.4,"total":1}},"seq":1,"timestamp":2}"#,
        r#"{"kind":"record","type":"usage","id":"r","lane":"main","cause":"adjustment","details":{"reason":"correction"},"usage":{"input":1,"output":2,"cacheRead":3,"cacheWrite":4,"totalTokens":10,"cost":{"input":0.1,"output":0.2,"cacheRead":0.3,"cacheWrite":0.4,"total":1}},"seq":1,"timestamp":2}"#,
    ];
    for line in cases {
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(encoded(value), format!("{line}\n"));
    }
}

#[test]
fn native_messages_and_payload_keys_round_trip_in_durable_records() {
    use pi_js_compat::js_json::parse_lossless;
    let raw_message = r#"{"role":"providerFailure","errorMessage":"\ud800!\udfff","\ud800":{"\udfff":"raw"}}"#;
    let entry = format!(r#"{{"type":"message","id":"message","message":{raw_message}}}"#);
    let bodies = [
        format!(r#""type":"operation_started","id":"r","lane":"main","sourceLeafId":null,"intent":{{"kind":"run","originalPrompt":[{raw_message}],"initialMessages":[{entry}]}}"#),
        format!(r#""type":"queue_enqueued","id":"r","lane":"main","queue":"nextRun","target":{entry}"#),
        format!(r#""type":"write_deferred","id":"r","lane":"main","runId":"run","target":{entry}"#),
        r#""type":"operation_finished","id":"r","lane":"main","runId":"run","outcome":"failed","error":{"code":"provider","message":"\ud800!\udfff"}"#.to_owned(),
        r#""type":"tool_started","id":"r","lane":"main","runId":"run","assistantEntryId":"a","toolIndex":0,"toolCallId":"call","toolName":"read","effectiveArgs":{"\ud800":"\udfff"},"resultEntryId":"result","replay":"safe""#.to_owned(),
    ];
    for body in bodies {
        let input = format!(r#"{{"kind":"record",{body},"seq":1,"timestamp":2}}"#);
        let mutation = parse_mutation(&input).unwrap();
        let output = encode_mutation(&mutation).unwrap();
        assert_eq!(parse_lossless(&output).unwrap(), parse_lossless(&input).unwrap());
    }
}

#[test]
fn seq_and_timestamp_must_be_javascript_safe_integers() {
    // `Number.isSafeInteger` rejects 2^53 and non-integral numbers; TS accepts
    // integral floats such as 1.0 because they are still integers.
    let line = |seq: &str, timestamp: &str| {
        format!(
            r#"{{"kind":"entry","lane":"main","type":"custom","id":"entry-1","customType":"note","data":{{}},"parentId":null,"seq":{seq},"timestamp":{timestamp}}}"#
        )
    };
    assert!(parse_mutation(&line("9007199254740991", "0")).is_ok());
    let float_case = parse_mutation(&line("1.0", "0.0"));
    assert!(float_case.is_ok(), "{float_case:?}");
    for seq in ["9007199254740992", "1.5", "1e20"] {
        let error = parse_mutation(&line(seq, "0")).expect_err(seq);
        assert_eq!(error.message, "has invalid seq", "{seq}");
    }
    for timestamp in ["9007199254740992", "0.5", "1e20"] {
        let error = parse_mutation(&line("1", timestamp)).expect_err(timestamp);
        assert_eq!(error.message, "has invalid timestamp", "{timestamp}");
    }
}
