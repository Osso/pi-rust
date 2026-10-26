use super::{
    native_prefix,
    session_records::{self, RecordWriter},
    sessions::SessionStore,
};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::path::Path;

#[test]
fn claude_bridge_import_repairs_orphan_and_interleaved_tool_pairs_before_writing() {
    let fixture = js_json::parse_lossless(include_str!("fixtures/repair-oracle.json")).unwrap();
    for case in fixture.get("imports").unwrap().as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SessionStore::at(dir.path().join("cc"), dir.path().join("agent"));
        let mut messages: Vec<pi_ai::types::Message> = js_json::from_value(case.get("input").unwrap()).unwrap();
        messages.push(pi_ai::types::Message::User(pi_ai::types::UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: pi_ai::types::UserContent::Text("current".into()),
            input_source: None,
            provider_native: None,
            timestamp: 3.0,
        }));
        let context = pi_ai::types::Context {
            system_prompt: None,
            tools: None,
            messages,
        };
        let id = store
            .sync(&"repair-fixture".into(), &context, dir.path(), "m", true)
            .unwrap()
            .unwrap();
        let path = session_records::session_path(&store.claude_dir, dir.path(), &id).unwrap();
        let records = session_records::read_records(&path).unwrap();
        let normalize = |mut value: JsValue| {
            if let JsValue::Object(fields) = &mut value {
                fields.shift_remove(&JsString::from("id"));
            }
            value
        };
        let actual = records
            .iter()
            .map(|r| normalize(r.get("message").unwrap().clone()))
            .collect::<Vec<_>>();
        let expected = case
            .get("messages")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .cloned()
            .map(normalize)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{:?}", case.get("name"));
    }
}

#[test]
fn claude_bridge_conversion_logs_exact_pre_repair_stats_and_repair_length() {
    let fixture = js_json::parse_lossless(include_str!("fixtures/repair-oracle.json")).unwrap();
    let mut input = fixture.get("imports").unwrap().as_array().unwrap()[0]
        .get("input")
        .unwrap()
        .clone();
    let blocks = input.as_array_mut().unwrap()[0]
        .get_mut("content")
        .unwrap()
        .as_array_mut()
        .unwrap();
    blocks[0]
        .as_object_mut()
        .unwrap()
        .insert("id".into(), JsValue::String("a|1".into()));
    blocks.push(JsValue::object([
        ("type", JsValue::String("thinking".into())),
        ("thinking", JsValue::String("private reasoning".into())),
    ]));
    let mut messages: Vec<pi_ai::types::Message> = js_json::from_value(&input).unwrap();
    messages.push(pi_ai::types::Message::User(pi_ai::types::UserMessage {
        role: pi_ai::types::UserRole::Tag,
        content: pi_ai::types::UserContent::Text("current".into()),
        input_source: None,
        provider_native: None,
        timestamp: 3.0,
    }));
    let dir = tempfile::tempdir().unwrap();
    let mut store = SessionStore::at(dir.path().join("cc"), dir.path().join("agent"));
    let mut logs = super::logging::Logs::new().unwrap();
    logs.enabled = true;
    store.verification_logs = Some(logs.clone());
    store
        .sync(
            &"convert-log-fixture".into(),
            &pi_ai::types::Context {
                system_prompt: None,
                tools: None,
                messages,
            },
            dir.path(),
            "m",
            true,
        )
        .unwrap();
    let text = std::fs::read_to_string(super::logging::debug_path().unwrap()).unwrap();
    let lines = text
        .lines()
        .filter_map(|line| {
            line.split_once(&format!("[{}] ", logs.module_id))
                .map(|(_, message)| message)
        })
        .filter(|line| line.starts_with("convertAndImportMessages:"))
        .collect::<Vec<_>>();
    assert_eq!(
        lines,
        vec![
            "convertAndImportMessages: 2 pi msgs → 2 anthropic msgs",
            "convertAndImportMessages: imported roles: [0]assistant:tool_use [1]assistant:text",
            "convertAndImportMessages: dropped 1 thinking (other)",
            "convertAndImportMessages: sanitized 1 tool IDs: a|1→a_1",
            "convertAndImportMessages: repairToolPairing 2 → 3 msgs",
        ]
    );
}

#[test]
fn cc_session_records_match_frozen_writer_and_round_trip_through_ts_reader() {
    let fixture = js_json::parse_lossless(include_str!("fixtures/session-oracle.json")).unwrap();
    let messages = fixture.get("provider").unwrap().as_array().unwrap();
    let frozen = fixture.get("records").unwrap().as_array().unwrap();
    let mut writer = RecordWriter::new(
        "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        Path::new("/fixture/project"),
        "claude-opus-5-5[1m]",
        Vec::new(),
    )
    .unwrap();
    for message in messages {
        writer.import_message(message).unwrap();
    }
    let mut ids = std::collections::HashMap::new();
    for (native, expected) in writer.records.iter().zip(frozen) {
        ids.insert(
            native.get("uuid").unwrap().clone().as_string().unwrap().clone(),
            expected.get("uuid").unwrap().clone(),
        );
    }
    let mut normalized = writer.records.clone();
    for (native, expected) in normalized.iter_mut().zip(frozen) {
        let JsValue::Object(fields) = native else {
            panic!("record");
        };
        for key in ["uuid", "parentUuid"] {
            if let Some(id) = fields.get(key).and_then(JsValue::as_string) {
                fields.insert(key.into(), ids[id].clone());
            }
        }
        for key in ["slug", "timestamp"] {
            fields.insert(key.into(), expected.get(key).unwrap().clone());
        }
        if let Some(JsValue::String(id)) = fields.get("requestId") {
            assert_eq!(id.len(), 32);
            fields.insert("requestId".into(), expected.get("requestId").unwrap().clone());
        }
        if let Some(JsValue::Object(message)) = fields.get_mut("message")
            && let Some(JsValue::String(id)) = message.get("id")
        {
            assert_eq!(id.len(), 32);
            message.insert("id".into(), expected.get("message").unwrap().get("id").unwrap().clone());
        }
        assert_eq!(
            js_json::stringify_lossless(&JsValue::Object(fields.clone())),
            js_json::stringify_lossless(expected)
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native.jsonl");
    session_records::write_records(&path, &writer.records).unwrap();
    assert_eq!(session_records::read_records(&path).unwrap(), writer.records);
    std::fs::write(
        dir.path().join("cc-session-io.mjs"),
        include_str!("fixtures/cc-session-io.mjs"),
    )
    .unwrap();
    let script = "import {readSession,serializeJsonl} from './cc-session-io.mjs';process.stdout.write(serializeJsonl(readSession(process.argv[1]).records));";
    let output = std::process::Command::new("node")
        .current_dir(dir.path())
        .args(["--input-type=module", "-e", script])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8(output.stderr).unwrap());
    let returned = String::from_utf8(output.stdout).unwrap();
    let records: Vec<JsValue> = returned
        .lines()
        .map(|line| js_json::parse_lossless(line).unwrap())
        .collect();
    assert_eq!(records, writer.records);
    std::fs::write(
        &path,
        fixture.get("jsonl").unwrap().as_string().unwrap().to_utf8().unwrap(),
    )
    .unwrap();
    assert_eq!(session_records::read_records(&path).unwrap(), *frozen);
}

#[test]
fn native_prefix_keeps_parallel_results_and_file_attachments_but_not_unsettled_tools() {
    let fixture = js_json::parse_lossless(include_str!("fixtures/session-oracle.json")).unwrap();
    let records = fixture.get("records").unwrap().as_array().unwrap();
    let messages = fixture.get("provider").unwrap().as_array().unwrap();
    assert_eq!(
        native_prefix::native_prefix(records, messages),
        (records.clone(), messages.len())
    );
    assert_eq!(native_prefix::native_prefix(records, &messages[..1]), (Vec::new(), 0));
    let native = js_json::parse_lossless(r#"[{"type":"user","uuid":"u","message":{"content":"@file"}},{"type":"attachment","uuid":"a","parentUuid":"u","attachment":{"type":"skill_listing"}},{"type":"attachment","uuid":"f","parentUuid":"a","attachment":{"type":"file","filename":"x","content":"\udfff"}}]"#).unwrap();
    let native = native.as_array().unwrap();
    let messages = js_json::parse_lossless(r#"[{"role":"user","content":"@file"}]"#).unwrap();
    let messages = messages.as_array().unwrap();
    let (placed, skipped) = native_prefix::tail_attachments(native, &[], messages, 0);
    assert!(skipped.is_empty());
    assert_eq!(
        placed[&0][0].get("content").unwrap().as_string().unwrap().units(),
        &[0xdfff]
    );
    assert_eq!(native_prefix::native_prefix(native, messages), (native.clone(), 1));
}

#[test]
fn claude_bridge_reuse_advances_cursor_past_each_trailing_assistant_without_rewriting_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("cc");
    let mut store = SessionStore::at(root.clone(), dir.path().join("agent"));
    let key = JsString::from("owned");
    store.record(&key, "native-id".into(), 1, dir.path()).unwrap();
    let first: pi_ai::types::Context = js_json::from_value(&js_json::parse_lossless(r#"{"messages":[{"role":"user","content":"u","timestamp":1},{"role":"assistant","api":"x","provider":"x","model":"x","content":[{"type":"text","text":"a1"}],"stopReason":"stop","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"timestamp":2},{"role":"user","content":"next","timestamp":3}]}"#).unwrap()).unwrap();
    assert_eq!(
        store.sync(&key, &first, dir.path(), "m", true).unwrap(),
        Some("native-id".into())
    );
    let mut second = first.clone();
    let assistant = second.messages[1].clone();
    second.messages.insert(2, assistant);
    assert_eq!(
        store.sync(&key, &second, dir.path(), "m", true).unwrap(),
        Some("native-id".into())
    );
    assert!(!root.exists(), "REUSE must not rewrite the native transcript");
}

#[test]
fn session_associations_survive_restart_and_rotate_on_abort() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path();
    let mut store = SessionStore::at(dir.path().join("cc"), dir.path().join("agent"));
    let context: pi_ai::types::Context = js_json::from_value(&js_json::parse_lossless(r#"{"messages":[{"role":"user","content":"history","timestamp":1},{"role":"user","content":"next","timestamp":2}]}"#).unwrap()).unwrap();
    let key = JsString::from("pi/session");
    store.record(&key, "old-id".into(), 0, cwd).unwrap();
    let association: JsValue = js_json::parse_lossless(
        std::fs::read_to_string(dir.path().join("agent/claude-bridge-sessions/pi%2Fsession.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(association.get("sessionId").unwrap().as_string().unwrap(), "old-id");
    // A trailing run of user messages belongs entirely to the current prompt.
    assert_eq!(
        store.sync(&key, &context, cwd, "model", true).unwrap(),
        Some("old-id".into())
    );
    store.mark_rebuild(&key, true);
    assert_eq!(store.sync(&key, &context, cwd, "model", true).unwrap(), None);
}
