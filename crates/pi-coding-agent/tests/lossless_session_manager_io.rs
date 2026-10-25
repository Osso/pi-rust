#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_manager::{ForkEntrySelection, SessionManager, load_entries_from_file};
use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8};

fn request(source: &mut oracle::TsOracle, operation: &str, path: &JsString, content: Option<&str>) -> JsValue {
    let mut fields = JsObject::new();
    fields.insert("id".into(), JsValue::Number(92000.0));
    fields.insert("operation".into(), JsValue::String(operation.into()));
    fields.insert("path".into(), JsValue::String(path.clone()));
    if let Some(content) = content {
        fields.insert("content".into(), JsValue::String(content.into()));
    }
    let response = source.request(&JsValue::Object(fields));
    assert_eq!(response.get("ok"), Some(&JsValue::Bool(true)), "{response:?}");
    response.get("output").unwrap().clone()
}

fn fixture() -> String {
    [
        r#"{"\ud800":{"extra":null},"type":"session","version":3,"id":"session\ud800","timestamp":"2026-10-21T00:00:00.000Z","cwd":"/tmp/project\udfff","parentSession":null}"#,
        r#"{"extra":null,"type":"message","id":"root\ud800","parentId":null,"timestamp":"2026-10-21T00:00:00.000Z","message":{"role":"user","content":"body\udfff","timestamp":0,"\ud800":null}}"#,
        r#"{"type":"custom_message","id":"custom\udfff","parentId":"root\ud800","timestamp":"2026-10-21T00:00:01.000Z","customType":"kind\ud800","content":"text\udfff","display":true,"details":{"\ud800":[null,"\udfff"]},"extra":false}"#,
        r#"{"type":"label","id":"label\ud800","parentId":"custom\udfff","timestamp":"2026-10-21T00:00:02.000Z","targetId":"root\ud800","label":"bookmark\udfff","extra":null}"#,
        r#"{"type":"session_info","id":"name\udfff","parentId":"label\ud800","timestamp":"2026-10-21T00:00:03.000Z","name":"name\ud800"}"#,
        r#"{"type":"extension\ud800","id":"unknown\udfff","parentId":"name\udfff","timestamp":"2026-10-21T00:00:04.000Z","body":{"\udfff":"\ud800"},"details":null}"#,
    ].join("\n") + "\n"
}

#[test]
fn ts_file_load_lookup_tree_context_fork_and_write_remain_lossless() {
    let sandbox = tempfile::tempdir().unwrap();
    let dir: JsString = sandbox.path().display().to_string().into();
    let path = dir.clone() + "/source" + JsString::from_utf16(vec![0xd800]) + ".jsonl";
    let content = fixture();
    let mut source = oracle::TsOracle::spawn(
        "lossless-session-records-oracle.mjs",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    request(&mut source, "write", &path, Some(&content));
    let expected = request(&mut source, "read", &path, None);
    let mut manager = SessionManager::open(&path, Some(&dir), None).unwrap();
    assert_eq!(
        manager.get_session_id(),
        expected.get("id").and_then(JsValue::as_string).unwrap()
    );
    assert_eq!(manager.get_session_file(), Some(&path));
    assert_eq!(
        serde_json::to_string(manager.get_header().unwrap()).unwrap(),
        expected
            .get("header")
            .and_then(JsValue::as_string)
            .unwrap()
            .to_utf8()
            .unwrap()
    );
    assert_eq!(
        serde_json::to_string(&manager.get_tree()).unwrap(),
        expected
            .get("tree")
            .and_then(JsValue::as_string)
            .unwrap()
            .to_utf8()
            .unwrap()
    );
    let context = manager.build_session_context();
    let mut actual = JsObject::new();
    actual.insert(
        "messages".into(),
        pi_js_compat::js_json::to_value(&context.messages).unwrap(),
    );
    actual.insert("thinkingLevel".into(), JsValue::String(context.thinking_level));
    actual.insert(
        "model".into(),
        context
            .model
            .map(|model| pi_js_compat::js_json::to_value(&model).unwrap())
            .unwrap_or(JsValue::Null),
    );
    let actual = JsValue::Object(actual);
    let ts_context = pi_js_compat::js_json::parse_lossless(
        expected
            .get("context")
            .and_then(JsValue::as_string)
            .unwrap()
            .to_utf8()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual, ts_context);
    let root = JsString::from("root") + JsString::from_utf16(vec![0xd800]);
    assert!(manager.get_entry(&root).is_some());
    assert_eq!(
        manager.get_label(&root),
        Some(&(JsString::from("bookmark") + JsString::from_utf16(vec![0xdfff])))
    );
    let custom = JsString::from("custom") + JsString::from_utf16(vec![0xdfff]);
    let fork_dir = dir.clone() + "/forks";
    let fork = SessionManager::fork_from(
        &path,
        &dir,
        Some(&fork_dir),
        None,
        Some(ForkEntrySelection::From(custom)),
    )
    .unwrap();
    let fork_read = request(&mut source, "read", fork.get_session_file().unwrap(), None);
    let actual_lines: Vec<_> = fork
        .get_entries()
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect();
    assert_eq!(
        pi_js_compat::js_json::to_value(&actual_lines).unwrap(),
        *fork_read.get("entries").unwrap()
    );
    manager
        .append_custom_entry(
            &(JsString::from("new") + JsString::from_utf16(vec![0xd800])),
            Some(JsValue::String(JsString::from_utf16(vec![0xdfff]))),
        )
        .unwrap();
    let written = request(&mut source, "read", &path, None);
    let actual_lines: Vec<_> = manager
        .get_entries()
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect();
    assert_eq!(
        pi_js_compat::js_json::to_value(&actual_lines).unwrap(),
        *written.get("entries").unwrap()
    );
}

#[test]
fn fork_unknown_record_adds_parent_link_without_inventing_identity() {
    let sandbox = tempfile::tempdir().unwrap();
    let dir: JsString = sandbox.path().display().to_string().into();
    let path = dir.clone() + "/unknown.jsonl";
    let content = concat!(
        "{\"type\":\"session\",\"version\":3,\"id\":\"s\",\"timestamp\":\"t\",\"cwd\":\"/tmp\"}\n",
        "{\"type\":\"extension\\ud800\",\"body\":{\"\\udfff\":null}}\n"
    );
    let mut source = oracle::TsOracle::spawn(
        "lossless-session-records-oracle.mjs",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let written = source
        .request(&serde_json::json!({"id":92010,"operation":"write","path":path.to_utf8().unwrap(),"content":content}));
    assert_eq!(written["ok"], true, "{written:?}");
    let fork_dir = dir.clone() + "/fork";
    let fork = SessionManager::fork_from(&path, &dir, Some(&fork_dir), None, Some(ForkEntrySelection::All)).unwrap();
    let written = std::fs::read_to_string(encode_node_utf8(fork.get_session_file().unwrap())).unwrap();
    assert_eq!(
        written.lines().nth(1).unwrap(),
        r#"{"type":"extension\ud800","body":{"\udfff":null},"parentId":null}"#
    );
}

#[test]
fn tree_sort_does_not_replace_invalid_raw_timestamps_with_epoch_zero() {
    let sandbox = tempfile::tempdir().unwrap();
    let dir: JsString = sandbox.path().display().to_string().into();
    let path = dir.clone() + "/tree.jsonl";
    let initial = fixture();
    let mut lines: Vec<_> = initial.lines().take(2).collect();
    lines.push(r#"{"type":"custom","id":"first","parentId":"root\ud800","timestamp":"2026-10-21T00:00:01.000Z","customType":"x"}"#);
    lines.push(
        r#"{"type":"custom","id":"second","parentId":"root\ud800","timestamp":"invalid\udfff","customType":"x"}"#,
    );
    let mut source = oracle::TsOracle::spawn(
        "lossless-session-records-oracle.mjs",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    request(&mut source, "write", &path, Some(&(lines.join("\n") + "\n")));
    let expected = request(&mut source, "read", &path, None);
    let manager = SessionManager::open(&path, Some(&dir), None).unwrap();
    assert_eq!(
        serde_json::to_string(&manager.get_tree()).unwrap(),
        expected
            .get("tree")
            .and_then(JsValue::as_string)
            .unwrap()
            .to_utf8()
            .unwrap()
    );
}

#[test]
fn compressed_transcript_load_preserves_raw_records() {
    let sandbox = tempfile::tempdir().unwrap();
    let path: JsString = sandbox.path().join("source.jsonl.zst").display().to_string().into();
    let content = fixture();
    let compressed = zstd::stream::encode_all(content.as_bytes(), 0).unwrap();
    std::fs::write(encode_node_utf8(&path), compressed).unwrap();
    let entries = load_entries_from_file(&path).unwrap();
    let lines: Vec<_> = entries
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect();
    assert_eq!(lines.join("\n") + "\n", content);
}

#[test]
fn raw_provider_native_survives_compaction_context_and_rewrite_and_typed_derivation_errors() {
    let sandbox = tempfile::tempdir().unwrap();
    let dir: JsString = sandbox.path().display().to_string().into();
    let path = dir.clone() + "/compaction.jsonl";
    let initial = fixture();
    let mut lines: Vec<_> = initial.lines().take(2).collect();
    lines.push(r#"{"type":"compaction","id":"compact\ud800","parentId":"root\ud800","timestamp":"invalid\udfff","summary":"summary\ud800","firstKeptEntryId":"root\ud800","tokensBefore":5,"durationMs":null,"details":{"\ud800":null},"providerNative":{"provider":"provider\ud800","api":"api\udfff","format":"openai.responses.input","value":{"\udfff":[null,"\ud800"]}},"tail":true}"#);
    let content = lines.join("\n") + "\n";
    let mut source = oracle::TsOracle::spawn(
        "lossless-session-records-oracle.mjs",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    request(&mut source, "write", &path, Some(&content));
    let expected = request(&mut source, "read", &path, None);
    let mut manager = SessionManager::open(&path, Some(&dir), None).unwrap();
    let context = manager.build_session_context();
    let expected_context = pi_js_compat::js_json::parse_lossless(
        expected
            .get("context")
            .and_then(JsValue::as_string)
            .unwrap()
            .to_utf8()
            .unwrap(),
    )
    .unwrap();
    let messages = pi_js_compat::js_json::to_value(&context.messages).unwrap();
    assert_eq!(&messages, expected_context.get("messages").unwrap());
    let context = messages.as_array().unwrap();
    let native = context
        .iter()
        .find_map(|message| message.get("providerNative"))
        .expect("native payload must not be dropped");
    let error = pi_js_compat::js_json::from_value::<pi_ai::types::ProviderNativeContext>(native)
        .expect_err("opaque machine tags must error at typed derivation");
    assert!(!error.to_string().is_empty());
    manager.append_custom_entry(&JsString::from("persist"), None).unwrap();
    let written = std::fs::read_to_string(encode_node_utf8(&path)).unwrap();
    assert!(written.lines().any(|line| line == lines[2]));
    let id = JsString::from("compact") + JsString::from_utf16(vec![0xd800]);
    let summary = JsString::from("  changed") + JsString::from_utf16(vec![0xdfff]) + "  ";
    let mut update = JsObject::new();
    update.insert("id".into(), JsValue::Number(92011.0));
    update.insert("operation".into(), JsValue::String("update_summary".into()));
    update.insert("path".into(), JsValue::String(path.clone()));
    update.insert("entryId".into(), JsValue::String(id.clone()));
    update.insert("summary".into(), JsValue::String(summary.clone()));
    let expected = source.request(&JsValue::Object(update));
    assert_eq!(expected.get("ok"), Some(&JsValue::Bool(true)), "{expected:?}");
    manager.update_compaction_summary(&id, &summary).unwrap();
    let before: Vec<_> = manager
        .get_entries()
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect();
    assert_eq!(
        &pi_js_compat::js_json::to_value(&before).unwrap(),
        expected.get("output").unwrap()
    );
    std::fs::remove_file(encode_node_utf8(&path)).unwrap();
    std::fs::create_dir(encode_node_utf8(&path)).unwrap();
    assert!(
        manager
            .update_compaction_summary(&id, &JsString::from("rewrite fails"))
            .is_err()
    );
    let restored: Vec<_> = manager
        .get_entries()
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect();
    assert_eq!(restored, before);
}

#[test]
fn mutation_rejects_noncompaction_before_testing_empty_summary() {
    let sandbox = tempfile::tempdir().unwrap();
    let dir: JsString = sandbox.path().display().to_string().into();
    let path = dir.clone() + "/source.jsonl";
    std::fs::write(encode_node_utf8(&path), fixture()).unwrap();
    let mut manager = SessionManager::open(&path, Some(&dir), None).unwrap();
    let id = JsString::from("root") + JsString::from_utf16(vec![0xd800]);
    let error = manager
        .update_compaction_summary(&id, &JsString::default())
        .unwrap_err();
    assert_eq!(
        error.message(),
        &(JsString::from("Entry ") + &id + " is not a compaction")
    );
}
