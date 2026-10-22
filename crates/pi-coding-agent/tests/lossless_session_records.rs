//! Q18 record authority regressions. All file fixtures are private temporary files.
use pi_coding_agent::core::session_manager::{migrate_session_entries, parse_session_entries};

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
