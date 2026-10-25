#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_manager::{migrate_session_entries, parse_session_entries};
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

#[test]
fn native_parser_accepts_literal_unpaired_units_without_a_utf8_boundary() {
    use pi_js_compat::js_json::JsObject;
    let raw = JsString::from_utf16(vec![0xd800]);
    let content = JsString::from("{\"type\":\"custom\",\"id\":\"") + &raw + "\",\"" + &raw + "\":\"literal\"}";
    let mut input = JsObject::new();
    input.insert("id".into(), JsValue::Number(91030.0));
    input.insert("operation".into(), JsValue::String("parse".into()));
    input.insert("content".into(), JsValue::String(content.clone()));
    let mut source = oracle::TsOracle::spawn(
        "lossless-session-records-oracle.mjs",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let expected = source.request(&JsValue::Object(input));
    assert_eq!(expected.get("ok"), Some(&JsValue::Bool(true)), "{expected:?}");
    let entries = parse_session_entries(&content);
    assert_eq!(entries.len(), 1);
    let actual = serde_json::to_string(&entries[0]).unwrap();
    assert_eq!(
        expected.get("output").unwrap().as_array().unwrap()[0].as_string(),
        Some(&JsString::from(actual))
    );
}

#[test]
fn pinned_parse_migrate_and_persistence_match_raw_record_lines() {
    use pi_coding_agent::core::session_tool_output::serialize_session_entry_for_persistence;
    let mut source = oracle::TsOracle::spawn(
        "lossless-session-records-oracle.mjs",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let content = concat!(
        "{\"extra\\ud800\":null,\"type\":\"session\",\"version\":2,\"id\":\"s\\ud800\",\"timestamp\":\"t\\udfff\",\"cwd\":null,\"parentSession\":null}\n",
        "{\"z\":false,\"type\":\"message\",\"id\":\"a\\ud800\",\"parentId\":null,\"timestamp\":\"t\\udfff\",\"message\":{\"role\":\"hookMessage\",\"content\":\"\\ud800\",\"timestamp\":0,\"\\udfff\":null}}\n",
        "{\"type\":\"unknown\\ud800\",\"id\":\"z\\udfff\",\"body\":{\"\\ud800\":[null,\"\\udfff\"]},\"details\":null}\n"
    );
    for (index, operation) in ["parse", "migrate", "serialize"].iter().enumerate() {
        let expected =
            source.request(&serde_json::json!({"id": 91000 + index,"operation":operation,"content":content}));
        assert_eq!(expected["ok"], true, "{expected:?}");
        let mut entries = parse_session_entries(content);
        if *operation == "migrate" {
            migrate_session_entries(&mut entries).unwrap();
        }
        let actual: Vec<String> = entries
            .iter()
            .map(|entry| {
                if *operation == "serialize" {
                    serialize_session_entry_for_persistence(entry).unwrap()
                } else {
                    serde_json::to_string(entry).unwrap()
                }
            })
            .collect();
        assert_eq!(serde_json::json!(actual), expected["output"]);
    }
}

#[test]
fn v1_migration_uses_target_record_identity_at_the_time_of_mutation() {
    let content = concat!(
        "{\"type\":\"session\",\"version\":1,\"id\":\"s\",\"timestamp\":\"t\"}\n",
        "{\"type\":\"compaction\",\"summary\":\"s\\ud800\",\"firstKeptEntryIndex\":2,\"tokensBefore\":1}\n",
        "{\"type\":\"extension\\ud800\",\"id\":\"before\\ud800\",\"firstKeptEntryIndex\":0,\"details\":null}"
    );
    let mut source = oracle::TsOracle::spawn(
        "lossless-session-records-oracle.mjs",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let expected = source.request(&serde_json::json!({"id":91010,"operation":"migrate","content":content}));
    assert_eq!(expected["ok"], true, "{expected:?}");
    let mut entries = parse_session_entries(content);
    migrate_session_entries(&mut entries).unwrap();
    let actual = parse_lossless(&serde_json::to_string(&entries[1]).unwrap()).unwrap();
    let ts = parse_lossless(expected["output"][1].as_str().unwrap()).unwrap();
    assert_eq!(actual.get("firstKeptEntryId"), ts.get("firstKeptEntryId"));
    let extension = parse_lossless(&serde_json::to_string(&entries[2]).unwrap()).unwrap();
    assert_eq!(extension.get("firstKeptEntryIndex"), Some(&JsValue::Number(0.0)));
}

#[test]
fn old_pending_deferred_and_end_turn_fields_load_and_save_as_frozen_raw_records() {
    use pi_coding_agent::core::session_tool_output::serialize_session_entry_for_persistence;
    let content = concat!(
        "{\"type\":\"session\",\"version\":3,\"id\":\"s\",\"timestamp\":\"2026-10-22T00:00:00.000Z\",\"cwd\":\"/tmp\"}\n",
        "{\"type\":\"message\",\"id\":\"p\",\"parentId\":null,\"timestamp\":\"t\",\"message\":{\"role\":\"assistant\",\"content\":[],\"stopReason\":\"pending\",\"deferred\":{\"id\":\"old-handle\"},\"extra\":\"keep\"}}\n",
        "{\"type\":\"message\",\"id\":\"d\",\"parentId\":\"p\",\"timestamp\":\"t\",\"message\":{\"role\":\"assistant\",\"content\":[],\"stopReason\":\"deferred\",\"deferred\":{\"id\":\"old-handle-2\"}}}\n",
        "{\"type\":\"message\",\"id\":\"e\",\"parentId\":\"d\",\"timestamp\":\"t\",\"message\":{\"role\":\"assistant\",\"content\":[],\"api\":\"openai-responses\",\"provider\":\"openai\",\"model\":\"test\",\"usage\":{\"input\":0,\"output\":0,\"cacheRead\":0,\"cacheWrite\":0,\"totalTokens\":0,\"cost\":{\"input\":0,\"output\":0,\"cacheRead\":0,\"cacheWrite\":0,\"total\":0}},\"stopReason\":\"stop\",\"timestamp\":0,\"endTurn\":true}}\n"
    );
    let mut source = oracle::TsOracle::spawn(
        "lossless-session-records-oracle.mjs",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let expected = source.request(&serde_json::json!({"id": 91040, "operation": "serialize", "content": content}));
    assert_eq!(expected["ok"], true, "{expected:?}");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("raw.jsonl");
    std::fs::write(&path, content).unwrap();
    let loaded = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<String> = parse_session_entries(loaded)
        .iter()
        .map(serialize_session_entry_for_persistence)
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(serde_json::json!(lines), expected["output"]);
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
}

#[test]
fn old_rust_tool_result_usage_survives_lossless_disk_read_and_rewrite() {
    use pi_coding_agent::core::session_tool_output::serialize_session_entry_for_persistence;
    let original = concat!(
        "{\"type\":\"session\",\"version\":3,\"id\":\"s\",\"timestamp\":\"2026-10-25T00:00:00.000Z\",\"cwd\":\"/tmp\"}\n",
        "{\"type\":\"message\",\"id\":\"tool\",\"parentId\":null,\"timestamp\":\"t\",\"message\":{\"role\":\"toolResult\",\"toolCallId\":\"call\",\"toolName\":\"read\",\"content\":[{\"type\":\"text\",\"text\":\"legacy output\"}],\"usage\":{\"input\":7,\"output\":3,\"cacheRead\":0,\"cacheWrite\":0,\"totalTokens\":10,\"cost\":{\"input\":0,\"output\":0,\"cacheRead\":0,\"cacheWrite\":0,\"total\":0}},\"addedToolNames\":[\"late_tool\"],\"isError\":false,\"timestamp\":0}}\n"
    );
    let mut source = oracle::TsOracle::spawn(
        "lossless-session-records-oracle.mjs",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let expected = source.request(&serde_json::json!({"id": 91050, "operation": "serialize", "content": original}));
    assert_eq!(expected["ok"], true, "{expected:?}");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("old-rust.jsonl");
    std::fs::write(&path, original.as_bytes()).unwrap();
    let loaded = std::fs::read_to_string(&path).unwrap();
    let lines = parse_session_entries(loaded)
        .iter()
        .map(serialize_session_entry_for_persistence)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(serde_json::json!(lines), expected["output"]);
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original.as_bytes());
}

#[test]
fn persistence_serializer_keeps_raw_header() {
    use pi_coding_agent::core::session_tool_output::serialize_session_entry_for_persistence;
    let line = r#"{"type":"session","id":"\ud800","timestamp":"\udfff","cwd":null,"\ud800":{"extra":"\udfff"}}"#;
    let entry = parse_session_entries(line).remove(0);
    assert_eq!(serialize_session_entry_for_persistence(&entry).unwrap(), line);
}

#[test]
fn truncating_imported_tool_result_preserves_unknown_entry_and_message_fields() {
    use pi_coding_agent::core::session_tool_output::{
        MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES, serialize_session_entry_for_persistence,
    };
    let mut line = String::from(
        r#"{"\ud800":"entry","type":"message","id":"id\ud800","parentId":null,"timestamp":"t","message":{"\udfff":"message","role":"toolResult","toolCallId":"t","toolName":"n","content":[{"type":"text","extra":"\ud800","text":""#,
    );
    line.push_str(&"x".repeat(MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES + 100));
    line.push_str(r#""}],"isError":false,"timestamp":0}}"#);
    let entry = parse_session_entries(&line).remove(0);
    let written = serialize_session_entry_for_persistence(&entry).unwrap();
    assert!(written.len() <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES);
    let raw = parse_lossless(&written).unwrap();
    assert_eq!(
        raw.as_object()
            .unwrap()
            .get(&JsString::from_utf16(vec![0xd800]))
            .and_then(JsValue::as_string),
        Some(&JsString::from("entry"))
    );
    assert_eq!(
        raw.get("message")
            .unwrap()
            .as_object()
            .unwrap()
            .get(&JsString::from_utf16(vec![0xdfff]))
            .and_then(JsValue::as_string),
        Some(&JsString::from("message"))
    );
    assert_eq!(serde_json::to_string(&entry).unwrap(), line);
    let mut source = oracle::TsOracle::spawn(
        "lossless-session-records-oracle.mjs",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let expected = source.request(&serde_json::json!({"id":91020,"operation":"serialize","content":line}));
    assert_eq!(expected["ok"], true, "{expected:?}");
    assert_eq!(expected["output"][0].as_str(), Some(written.as_str()));
}
