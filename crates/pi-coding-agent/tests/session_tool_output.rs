use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue, encode_node_utf8};
use std::fs;
use std::path::{Path, PathBuf};

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, TextOrImageContent, ToolResultMessage,
    ToolResultRole, Usage,
};
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry, SessionManager, SessionMessageEntry};
use pi_coding_agent::core::session_tool_output::{
    MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES, TOOL_RESULT_TRUNCATION_MARKER, migrate_tool_result_session_files,
    truncate_tool_result_for_persistence,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pi-rust-{name}-{}",
        pi_ai::utils::random_uuid::random_uuid().unwrap()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn tool_result(text: &str) -> AgentMessage {
    AgentMessage::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "call-1".to_owned(),
        tool_name: "bash".to_owned(),
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: None,
        is_error: false,
        timestamp: 1.0,
    })
}

fn tool_result_entry(text: &str) -> SessionEntry {
    SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        mailbox_store_refs: None,
        id: "tool-1".to_owned().into(),
        parent_id: None,
        timestamp: "2025-01-01T00:00:00.000Z".to_owned().into(),
        message: Box::new(tool_result(text)),
    }))
}

fn text_from_tool_result(entry: &SessionEntry) -> JsString {
    let message = entry.as_message().unwrap();
    match message.message.as_ref() {
        AgentMessage::ToolResult(result) => match &result.content[0] {
            TextOrImageContent::Text(text) => text.text.clone(),
            TextOrImageContent::Image(_) => panic!("expected text tool result"),
        },
        other => panic!("expected tool result, got {other:?}"),
    }
}

fn assistant_message(text: &str) -> AgentMessage {
    AgentMessage::Assistant(Box::new(AssistantMessage {
        role: AssistantRole::Tag,
        content: vec![AssistantContent::Text(TextContent::new(text))],
        api: "anthropic-messages".to_owned(),
        provider: "anthropic".to_owned(),
        model: "test".to_owned(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 1.0,
    }))
}

fn write_session(path: &Path, message: AgentMessage) {
    let assistant =
        pi_coding_agent::core::session_manager::SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
            mailbox_store_refs: None,
            id: "assistant-1".to_owned().into(),
            parent_id: None,
            timestamp: "2025-01-01T00:00:00.000Z".to_owned().into(),
            message: Box::new(assistant_message("assistant")),
        }));
    let tool =
        pi_coding_agent::core::session_manager::SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
            mailbox_store_refs: None,
            id: "tool-1".to_owned().into(),
            parent_id: Some("assistant-1".to_owned().into()),
            timestamp: "2025-01-01T00:00:00.000Z".to_owned().into(),
            message: Box::new(message),
        }));
    let header = pi_coding_agent::core::session_manager::SessionHeader::new(
        "session-1".into(),
        "2025-01-01T00:00:00.000Z".into(),
        Some("/tmp".into()),
        None,
    );
    let lines = [
        pi_coding_agent::core::session_manager::FileEntry::Header(header),
        pi_coding_agent::core::session_manager::FileEntry::Entry(assistant),
        pi_coding_agent::core::session_manager::FileEntry::Entry(tool),
    ]
    .iter()
    .map(|entry| serde_json::to_string(entry).unwrap())
    .collect::<Vec<_>>()
    .join("\n");
    fs::write(path, format!("{lines}\n")).unwrap();
}

#[test]
fn caps_single_line_tool_result_on_disk_without_changing_the_runtime_entry() {
    let dir = temp_dir("tool-output-cap");
    let mut session = SessionManager::create(
        &JsString::from(&dir.to_str().expect("utf-8 test path").to_owned()),
        Some(&JsString::from(&dir.to_str().expect("utf-8 test path").to_owned())),
        None,
    )
    .unwrap();
    let original = "x".repeat(MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES + 100);
    session.append_message(assistant_message("assistant")).unwrap();
    let entry_id = session.append_message(tool_result(&original)).unwrap();
    let file = session.get_session_file().unwrap().to_owned();
    let content = fs::read_to_string(&encode_node_utf8(&file)).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let persisted_index = lines
        .iter()
        .position(|line| {
            js_json::parse_lossless(*line).ok().is_some_and(|entry| {
                entry["type"] == json!("message") && entry["message"]["role"] == json!("toolResult")
            })
        })
        .unwrap();
    let persisted: JsValue = js_json::parse_lossless(lines[persisted_index]).unwrap();
    let persisted_text = persisted["message"]["content"][0]["text"].as_string().unwrap();
    assert!(encode_node_utf8(persisted_text).len() <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES);
    assert!(lines[persisted_index].len() <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES);
    assert!(persisted_text.contains(TOOL_RESULT_TRUNCATION_MARKER));

    let runtime = session.get_entry(&entry_id).unwrap();
    assert_eq!(text_from_tool_result(runtime), original);
}

#[test]
fn persisted_details_use_javascript_key_order_and_numbers() {
    let mut message = tool_result("ok");
    let AgentMessage::ToolResult(result) = &mut message else {
        unreachable!()
    };
    result.details = Some(js_json::parse_lossless(r#"{"2":2.0,"1":1.0,"x":3.0}"#).unwrap());
    let entry = SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        mailbox_store_refs: None,
        id: "tool-1".into(),
        parent_id: None,
        timestamp: "2025-01-01T00:00:00.000Z".into(),
        message: Box::new(message),
    }));
    let line = pi_coding_agent::core::session_tool_output::serialize_session_entry(&entry).unwrap();
    assert!(line.contains(r#""details":{"1":1,"2":2,"x":3}"#), "{line}");
}

#[test]
fn truncates_multibyte_utf8_at_a_valid_boundary() {
    let entry = tool_result_entry(&"😀漢字".repeat(MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES));
    let (persisted, changed) = truncate_tool_result_for_persistence(&entry).unwrap();
    assert!(changed);
    let text = text_from_tool_result(&persisted);
    assert!(encode_node_utf8(&text).len() <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES);
    assert!(text.contains(TOOL_RESULT_TRUNCATION_MARKER));
    assert!(!text.contains('\u{fffd}'));
}

#[test]
fn does_not_emit_replacement_character_at_multibyte_boundary() {
    let entry = tool_result_entry(&"😀".repeat(MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES));
    let (persisted, _) = truncate_tool_result_for_persistence(&entry).unwrap();
    let text = text_from_tool_result(&persisted);
    assert!(!text.contains('\u{fffd}'));
}

#[test]
fn returns_unchanged_small_tool_results() {
    let entry = tool_result_entry("small output");
    let (persisted, changed) = truncate_tool_result_for_persistence(&entry).unwrap();
    assert!(!changed);
    assert_eq!(persisted, entry);
}

#[test]
fn migrates_oversized_sessions_atomically_and_idempotently() {
    let agent_dir = temp_dir("tool-output-migration");
    let session_dir = agent_dir.join("sessions").join("project");
    let session_path = session_dir.join("session.jsonl");
    fs::create_dir_all(&session_dir).unwrap();
    write_session(
        &session_path,
        tool_result(&"y".repeat(MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES + 1)),
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&session_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let malformed_path = agent_dir.join("broken.jsonl");
    fs::write(&malformed_path, "not json\n").unwrap();
    let non_session_path = agent_dir.join("other.jsonl");
    fs::write(&non_session_path, "{\"type\":\"event\"}\n").unwrap();

    let first = migrate_tool_result_session_files(&agent_dir.to_str().expect("utf-8 test path").into(), || {
        Ok(1_700_000_000_000.0)
    })
    .unwrap();
    assert_eq!(first.changed_files, 1);
    assert_eq!(first.truncated_messages, 1);
    assert_eq!(first.skipped_malformed_files, 1);
    assert_eq!(first.skipped_non_session_files, 1);
    let backups: Vec<String> = fs::read_dir(&session_dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_str().expect("utf-8 test path").to_owned())
        .filter(|name| name.contains(".tool-output-backup-"))
        .collect();
    assert_eq!(backups.len(), 1);

    let content = fs::read_to_string(&session_path).unwrap();
    let migrated: JsValue = js_json::parse_lossless(content.lines().nth(2).unwrap()).unwrap();
    let migrated_text = migrated["message"]["content"][0]["text"].as_string().unwrap();
    assert!(encode_node_utf8(migrated_text).len() <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&session_path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    let second = migrate_tool_result_session_files(&agent_dir.to_str().expect("utf-8 test path").into(), || {
        Ok(1_700_000_000_000.0)
    })
    .unwrap();
    assert_eq!(second.changed_files, 0);
    assert_eq!(second.truncated_messages, 0);
    let backups_after: Vec<String> = fs::read_dir(&session_dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_str().expect("utf-8 test path").to_owned())
        .filter(|name| name.contains(".tool-output-backup-"))
        .collect();
    assert_eq!(backups_after.len(), 1);
    assert!(session_path.exists());
}

#[test]
fn skips_a_session_that_changes_during_migration() {
    let agent_dir = temp_dir("tool-output-changed");
    let session_dir = agent_dir.join("sessions").join("project");
    let session_path = session_dir.join("session.jsonl");
    fs::create_dir_all(&session_dir).unwrap();
    write_session(
        &session_path,
        tool_result(&"z".repeat(MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES + 1)),
    );

    let report = migrate_tool_result_session_files(&agent_dir.to_str().expect("utf-8 test path").into(), || {
        let mut content = fs::read_to_string(&session_path).unwrap();
        content.push('\n');
        fs::write(&session_path, content).unwrap();
        Ok(1_700_000_000_000.0)
    })
    .unwrap();

    assert_eq!(report.changed_files, 0);
    assert_eq!(report.skipped_error_files, 1);
    assert!(
        report.errors[0]
            .to_utf8()
            .unwrap()
            .contains("session changed while scanning")
    );
}
