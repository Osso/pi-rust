use pi_agent_core::agent::AgentMessage;
use pi_coding_agent::core::session_manager::{FileEntry, SessionEntry, parse_session_entries};
use pi_coding_agent::core::session_tool_output::serialize_session_entry;
use pi_js_compat::js_json::parse_lossless;

#[test]
fn prompt_template_arguments_retain_surrogates_and_quote_boundaries() {
    use pi_coding_agent::core::prompt_templates::{parse_command_args, substitute_args};
    use pi_js_compat::js_json::JsString;
    let input = JsString::from_utf16(vec![0x22, 0xd800, 0x20, 0xdfff, 0x22, 0x20, 0x78]);
    let args = parse_command_args(&input);
    assert_eq!(
        args,
        vec![JsString::from_utf16(vec![0xd800, 0x20, 0xdfff]), JsString::from("x")]
    );
    let expanded = substitute_args("[$1][$@]", &args);
    assert_eq!(
        expanded.units(),
        &[
            0x5b, 0xd800, 0x20, 0xdfff, 0x5d, 0x5b, 0xd800, 0x20, 0xdfff, 0x20, 0x78, 0x5d
        ]
    );
}

#[test]
fn session_jsonl_retains_message_summary_and_custom_units_on_reload() {
    for record in [
        r#"{"type":"message","id":"m","parentId":null,"timestamp":"2026-10-21T00:00:00Z","message":{"role":"toolResult","toolCallId":"c","toolName":"probe","content":[{"type":"text","text":"\ud800"}],"details":{"\udfff":"\ud800"},"isError":false,"timestamp":1}}"#,
        r#"{"type":"custom","id":"c","parentId":null,"timestamp":"2026-10-21T00:00:00Z","customType":"probe","data":{"\ud800":"\udfff"}}"#,
        r#"{"type":"compaction","id":"s","parentId":null,"timestamp":"2026-10-21T00:00:00Z","summary":"\ud800","firstKeptEntryId":"m","tokensBefore":4,"details":{"path":"\udfff"}}"#,
    ] {
        let header =
            r#"{"type":"session","version":3,"id":"lossless","timestamp":"2026-10-21T00:00:00Z","cwd":"/tmp"}"#;
        let parsed = parse_session_entries(&format!("{header}\n{record}\n"));
        let entry = parsed
            .iter()
            .find_map(|entry| match entry {
                FileEntry::Entry(entry) => Some(entry),
                _ => None,
            })
            .expect("valid record must not be discarded");
        assert!(
            matches!(entry, SessionEntry::Known(_)),
            "known record must remain typed"
        );
        let wire = serialize_session_entry(entry);
        assert_eq!(parse_lossless(&wire).unwrap(), parse_lossless(record).unwrap());
        let reloaded: SessionEntry = serde_json::from_str(&wire).unwrap();
        assert_eq!(&reloaded, entry);
        if let Some(message) = reloaded.as_message() {
            assert!(matches!(message.message.as_ref(), AgentMessage::ToolResult(_)));
        }
    }
}
