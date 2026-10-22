use pi_coding_agent::core::session_manager::parse_session_entries;
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

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
}
