//! Q18 record authority regressions. All file fixtures are private temporary files.
use pi_coding_agent::core::session_manager::{FileEntry, LeafSelection, build_context_entries, build_session_context};
use pi_coding_agent::core::session_manager::{migrate_session_entries, parse_session_entries};
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

#[test]
fn parsed_records_preserve_all_json_fields_and_utf16() {
    let lines = [
        r#"{"extra\ud800":{"\udfff":"\ud800"},"type":"session","version":3,"id":"id\ud800","timestamp":"time\udfff","cwd":"/tmp/\ud800","parentSession":null}"#,
        r#"{"z":null,"type":"message","id":"m\ud800","parentId":null,"timestamp":"\udfff","message":{"role":"user","content":"\ud800","timestamp":0},"\ud800":"\udfff"}"#,
        r#"{"type":"extension\ud800","id":"u\udfff","parentId":"m\ud800","body":{"\ud800":[null,"\udfff"]},"mailboxStoreRefs":null,"summary_extra":{"x":"\ud800"}}"#,
    ];
    let entries = parse_session_entries(&lines.join("\n"));
    assert_eq!(entries.len(), lines.len());
    for (entry, line) in entries.iter().zip(lines) {
        assert_eq!(serde_json::to_string(entry).unwrap(), line);
    }
}

#[test]
fn context_and_lookup_follow_raw_string_identity() {
    let entries = parse_session_entries(concat!(
        "{\"type\":\"message\",\"id\":\"root\\ud800\",\"parentId\":null,\"timestamp\":\"t\",\"message\":{\"role\":\"user\",\"content\":\"body\\ud800\",\"timestamp\":0}}\n",
        "{\"type\":\"message\",\"id\":\"leaf\\udfff\",\"parentId\":\"root\\ud800\",\"timestamp\":\"t\",\"message\":{\"role\":\"user\",\"content\":\"second\\udfff\",\"timestamp\":1}}"
    ));
    let entries: Vec<_> = entries
        .into_iter()
        .filter_map(|e| match e {
            FileEntry::Entry(e) => Some(e),
            _ => None,
        })
        .collect();
    let leaf = JsString::from_utf16(vec![108, 101, 97, 102, 0xdfff]);
    let path = build_context_entries(&entries, LeafSelection::Id(&leaf));
    assert_eq!(path.len(), 2);
    let context = build_session_context(&entries, LeafSelection::Id(&leaf));
    assert_eq!(context.messages.len(), 2);
    let raw = parse_lossless(&serde_json::to_string(&context.messages).unwrap()).unwrap();
    assert_eq!(
        raw.as_array().unwrap()[0]
            .get("content")
            .and_then(JsValue::as_string)
            .unwrap()
            .units(),
        &[98, 111, 100, 121, 0xd800]
    );
}

#[test]
fn parse_retains_non_record_json_without_inventing_a_wrapper() {
    let lines = ["null", "false", "42", r#""\ud800""#, r#"[null,{"\udfff":"\ud800"}]"#];
    let entries = parse_session_entries(&lines.join("\n"));
    assert_eq!(entries.len(), lines.len());
    for (entry, line) in entries.iter().zip(lines) {
        assert_eq!(serde_json::to_string(entry).unwrap(), line);
    }
}

#[test]
fn migration_mutates_raw_record_without_losing_extra_fields() {
    let lines = [
        r#"{"type":"session","version":2,"id":"s","timestamp":"t","cwd":"/tmp","\ud800":null}"#,
        r#"{"type":"message","id":"m\ud800","parentId":null,"timestamp":"t","message":{"role":"hookMessage","content":"\ud800","timestamp":0,"\udfff":null},"\ud800":"extra"}"#,
    ];
    let mut entries = parse_session_entries(&lines.join("\n"));
    migrate_session_entries(&mut entries);
    assert_eq!(
        serde_json::to_string(&entries[0]).unwrap(),
        lines[0].replace("\"version\":2", "\"version\":3")
    );
    assert_eq!(
        serde_json::to_string(&entries[1]).unwrap(),
        lines[1].replace("hookMessage", "custom")
    );
}
