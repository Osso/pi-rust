#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_manager::{migrate_session_entries, parse_session_entries};
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

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
            migrate_session_entries(&mut entries);
        }
        let actual: Vec<String> = entries
            .iter()
            .map(|entry| {
                if *operation == "serialize" {
                    serialize_session_entry_for_persistence(entry)
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
    migrate_session_entries(&mut entries);
    let actual = parse_lossless(&serde_json::to_string(&entries[1]).unwrap()).unwrap();
    let ts = parse_lossless(expected["output"][1].as_str().unwrap()).unwrap();
    assert_eq!(actual.get("firstKeptEntryId"), ts.get("firstKeptEntryId"));
    let extension = parse_lossless(&serde_json::to_string(&entries[2]).unwrap()).unwrap();
    assert_eq!(extension.get("firstKeptEntryIndex"), Some(&JsValue::Number(0.0)));
}

#[test]
fn persistence_serializer_keeps_raw_header() {
    use pi_coding_agent::core::session_tool_output::serialize_session_entry_for_persistence;
    let line = r#"{"type":"session","id":"\ud800","timestamp":"\udfff","cwd":null,"\ud800":{"extra":"\udfff"}}"#;
    let entry = parse_session_entries(line).remove(0);
    assert_eq!(serialize_session_entry_for_persistence(&entry), line);
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
    let written = serialize_session_entry_for_persistence(&entry);
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
