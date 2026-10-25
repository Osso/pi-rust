use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::encode_node_utf8;
use std::fs;
use std::path::{Path, PathBuf};

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, TextOrImageContent, ToolResultMessage,
    ToolResultRole, Usage, UserContent, UserMessage, UserRole,
};
use pi_coding_agent::core::session_control_db::{
    WritableSessionMetadata, read_session_goal, read_session_metadata, read_session_name_state, write_session_metadata,
    write_session_name,
};
use pi_coding_agent::core::session_manager::{
    CURRENT_SESSION_VERSION, FileEntry, ForkEntrySelection, KnownSessionEntry, LeafSelection, NewSessionOptions,
    SessionEntry, SessionManager, SessionMessageEntry, SessionTreeNode, SetMetadataControlDbPathOptions,
    assert_valid_session_id, build_context_entries, build_session_context, find_most_recent_session,
    get_default_session_dir, get_default_session_dir_path, load_entries_from_file, migrate_session_entries,
    parse_session_entries, read_session_header,
};

fn user_message(text: &str, timestamp: i64) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(JsString::from(text.to_owned())),
        input_source: None,
        provider_native: None,
        timestamp: timestamp as f64,
    })
}

fn assistant_message(text: &str, timestamp: i64) -> AgentMessage {
    AgentMessage::Assistant(Box::new(AssistantMessage {
        role: AssistantRole::Tag,
        content: vec![AssistantContent::Text(TextContent::new(text))],
        api: "anthropic-messages".to_owned(),
        provider: "anthropic".to_owned(),
        model: "claude-test".to_owned(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: timestamp as f64,
    }))
}

fn tool_result_message(tool_call_id: &str, text: &str) -> AgentMessage {
    AgentMessage::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: tool_call_id.to_owned(),
        tool_name: "pyrun_eval".to_owned(),
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: None,
        is_error: false,
        timestamp: 1.0,
    })
}

fn message_entry(id: &str, parent_id: Option<&str>, message: AgentMessage) -> SessionEntry {
    SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        mailbox_store_refs: None,
        id: JsString::from(id.to_owned()),
        parent_id: parent_id.map(str::to_owned).map(JsString::from),
        timestamp: JsString::from("2025-01-01T00:00:00Z"),
        message: Box::new(message),
    }))
}

fn user_entry(id: &str, parent_id: Option<&str>, text: &str) -> SessionEntry {
    message_entry(id, parent_id, user_message(text, 1))
}

fn assistant_entry(id: &str, parent_id: Option<&str>, text: &str) -> SessionEntry {
    message_entry(id, parent_id, assistant_message(text, 1))
}

fn compaction_entry(id: &str, parent_id: Option<&str>, summary: &str, first_kept: &str) -> SessionEntry {
    SessionEntry::Known(KnownSessionEntry::Compaction(
        pi_coding_agent::core::session_manager::CompactionEntry {
            id: JsString::from(id.to_owned()),
            parent_id: parent_id.map(str::to_owned).map(JsString::from),
            timestamp: JsString::from("2025-01-01T00:00:00Z"),
            summary: JsString::from(summary.to_owned()),
            first_kept_entry_id: JsString::from(first_kept.to_owned()),
            tokens_before: 1000.0,
            extra: Default::default(),
            duration_ms: None,
            details: None,
            provider_native: None,
            from_hook: None,
            first_kept_entry_index: None,
        },
    ))
}

fn ids(entries: &[SessionEntry]) -> Vec<JsString> {
    entries
        .iter()
        .map(|entry| entry.id().cloned().unwrap_or_default())
        .collect()
}

fn tree_ids(nodes: &[SessionTreeNode]) -> Vec<JsString> {
    let mut result = Vec::new();
    for node in nodes {
        result.push(node.entry.id().cloned().unwrap_or_default());
        result.extend(tree_ids(&node.children));
    }
    result
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pi-rust-{name}-{}",
        pi_ai::utils::random_uuid::random_uuid().unwrap()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_jsonl(path: &Path, entries: &[serde_json::Value]) {
    let content = entries
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, format!("{content}\n")).unwrap();
}

fn header_json(id: &str, cwd: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "session",
        "version": 3,
        "id": id,
        "timestamp": "2025-01-01T00:00:00Z",
        "cwd": cwd,
    })
}

#[test]
fn validates_session_ids() {
    for valid in ["abc", "a", "abc-123_def.456", "A1"] {
        assert_valid_session_id(&JsString::from(valid)).unwrap();
    }
    for invalid in [
        "", "-abc", "abc-", "_abc", "abc_", ".abc", "abc.", "abc/def", "abc\\def", "abc def",
    ] {
        let error = assert_valid_session_id(&JsString::from(invalid)).unwrap_err();
        assert!(error.to_string().starts_with("Session id must be non-empty"));
    }
}

#[test]
fn parses_entries_skipping_malformed_lines() {
    let content = "\n\n{\"type\":\"session\",\"id\":\"abc\",\"timestamp\":\"2025-01-01T00:00:00Z\",\"cwd\":\"/tmp\"}\nnot json\n{\"type\":\"message\",\"id\":\"1\",\"parentId\":null,\"timestamp\":\"2025-01-01T00:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"hi\",\"timestamp\":1}}\n";
    let entries = parse_session_entries(content);
    assert_eq!(entries.len(), 2);
    assert!(matches!(entries[0], FileEntry::Header(_)));
    assert_eq!(entries[1].entry().unwrap().id(), Some(&JsString::from("1")));
}

#[test]
fn migrates_v1_entries_and_is_idempotent() {
    let mut entries = parse_session_entries(
        "{\"type\":\"session\",\"id\":\"sess-1\",\"timestamp\":\"2025-01-01T00:00:00Z\",\"cwd\":\"/tmp\"}\n\
         {\"type\":\"message\",\"timestamp\":\"2025-01-01T00:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"hi\",\"timestamp\":1}}\n\
         {\"type\":\"message\",\"timestamp\":\"2025-01-01T00:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"hello\"}],\"api\":\"test\",\"provider\":\"test\",\"model\":\"test\",\"usage\":{\"input\":1,\"output\":1,\"cacheRead\":0,\"cacheWrite\":0},\"stopReason\":\"stop\",\"timestamp\":2}}\n",
    );
    migrate_session_entries(&mut entries).unwrap();
    let FileEntry::Header(header) = &entries[0] else {
        panic!("expected header");
    };
    assert_eq!(header.version(), Some(3.0));
    let first = entries[1].entry().unwrap();
    let second = entries[2].entry().unwrap();
    assert_eq!(first.id().unwrap().len(), 8);
    assert_eq!(first.parent_id(), None);
    assert_eq!(second.parent_id(), first.id());

    let before: Vec<Option<JsString>> = entries
        .iter()
        .map(|entry| entry.entry().and_then(|entry| entry.id().cloned()))
        .collect();
    migrate_session_entries(&mut entries).unwrap();
    let after: Vec<Option<JsString>> = entries
        .iter()
        .map(|entry| entry.entry().and_then(|entry| entry.id().cloned()))
        .collect();
    assert_eq!(before, after);
}

#[test]
fn builds_context_without_compaction() {
    let entries = vec![
        user_entry("1", None, "hello"),
        assistant_entry("2", Some("1"), "hi there"),
        user_entry("3", Some("2"), "how are you"),
        assistant_entry("4", Some("3"), "great"),
    ];
    let context = build_session_context(&entries, LeafSelection::Last);
    let roles: Vec<JsString> = context.messages.iter().map(AgentMessage::role).collect();
    assert_eq!(roles, ["user", "assistant", "user", "assistant"]);
    assert_eq!(context.thinking_level, "off");
    assert_eq!(
        context.model,
        Some(pi_coding_agent::core::session_manager::SessionModel {
            provider: JsString::from("anthropic"),
            model_id: JsString::from("claude-test"),
        })
    );

    let empty = build_session_context(&[], LeafSelection::Last);
    assert!(empty.messages.is_empty());
    assert_eq!(empty.thinking_level, "off");
    assert_eq!(empty.model, None);
}

#[test]
fn builds_compacted_context_with_summary_and_kept_messages() {
    let entries = vec![
        user_entry("1", None, "first"),
        assistant_entry("2", Some("1"), "response1"),
        user_entry("3", Some("2"), "second"),
        assistant_entry("4", Some("3"), "response2"),
        compaction_entry("5", Some("4"), "Summary of first two turns", "3"),
        user_entry("6", Some("5"), "third"),
        assistant_entry("7", Some("6"), "response3"),
    ];
    let context = build_session_context(&entries, LeafSelection::Last);
    assert_eq!(context.messages.len(), 5);
    assert_eq!(context.messages[0].role(), "compactionSummary");
    assert_eq!(context.messages[1], user_message("second", 1));

    let context_entries = build_context_entries(&entries, LeafSelection::Last);
    assert_eq!(ids(&context_entries), ["5", "3", "4", "6", "7"]);
}

#[test]
fn uses_latest_compaction_and_keeps_settings_from_full_path() {
    let entries = vec![
        user_entry("1", None, "a"),
        assistant_entry("2", Some("1"), "b"),
        compaction_entry("3", Some("2"), "First summary", "1"),
        user_entry("4", Some("3"), "c"),
        assistant_entry("5", Some("4"), "d"),
        compaction_entry("6", Some("5"), "Second summary", "4"),
        user_entry("7", Some("6"), "e"),
    ];
    let context = build_session_context(&entries, LeafSelection::Last);
    assert_eq!(context.messages.len(), 4);
    assert_eq!(context.messages[0].role(), "compactionSummary");
    assert_eq!(
        ids(&build_context_entries(&entries, LeafSelection::Last)),
        ["6", "4", "5", "7"]
    );
}

#[test]
fn follows_explicit_branch_and_includes_branch_summary() {
    let entries = vec![
        user_entry("1", None, "start"),
        assistant_entry("2", Some("1"), "response"),
        user_entry("3", Some("2"), "branch A"),
        user_entry("4", Some("2"), "branch B"),
    ];
    let context_a = build_session_context(&entries, LeafSelection::Id(&JsString::from("3")));
    assert_eq!(context_a.messages.len(), 3);
    assert_eq!(context_a.messages[2], user_message("branch A", 1));

    let entries = vec![
        user_entry("1", None, "start"),
        assistant_entry("2", Some("1"), "response"),
        user_entry("3", Some("2"), "abandoned path"),
        SessionEntry::Known(KnownSessionEntry::BranchSummary(
            pi_coding_agent::core::session_manager::BranchSummaryEntry {
                id: JsString::from("4"),
                parent_id: Some(JsString::from("2")),
                timestamp: JsString::from("2025-01-01T00:00:00Z"),
                from_id: JsString::from("3"),
                summary: JsString::from("Summary of abandoned work"),
                details: None,
                extra: Default::default(),
                from_hook: None,
            },
        )),
        user_entry("5", Some("4"), "new direction"),
    ];
    let context = build_session_context(&entries, LeafSelection::Id(&JsString::from("5")));
    assert_eq!(context.messages.len(), 4);
    assert_eq!(context.messages[2].role(), "branchSummary");
    assert_eq!(context.messages[3], user_message("new direction", 1));
}

#[test]
fn drops_orphaned_tool_results_from_capped_pre_compaction_suffix() {
    let tool_call_id = "call_with_openai_id|fc_123";
    let big_thinking = "x".repeat(25_000);
    let tool_call_message = AgentMessage::Assistant(Box::new(AssistantMessage {
        role: AssistantRole::Tag,
        content: vec![
            AssistantContent::Thinking(pi_ai::types::ThinkingContent {
                r#type: pi_ai::types::ThinkingTag::Tag,
                thinking: JsString::from(big_thinking),
                thinking_signature: Some("sig".to_owned()),
                redacted: None,
                index: None,
            }),
            AssistantContent::Text(TextContent::new("")),
            AssistantContent::ToolCall(pi_ai::types::ToolCall::new(
                tool_call_id,
                "pyrun_eval",
                serde_json::Map::new()
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
            )),
        ],
        api: "anthropic-messages".to_owned(),
        provider: "anthropic".to_owned(),
        model: "claude-test".to_owned(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage: Usage::default(),
        stop_reason: StopReason::ToolUse,
        error_message: None,
        timestamp: 1.0,
    }));
    let entries = vec![
        user_entry("1", None, "start"),
        message_entry("2", Some("1"), tool_call_message),
        message_entry("3", Some("2"), tool_result_message(tool_call_id, "ok")),
        assistant_entry("4", Some("3"), "done"),
        compaction_entry("5", Some("4"), "Summary", "1"),
    ];
    let context = build_session_context(&entries, LeafSelection::Last);
    let roles: Vec<JsString> = context.messages.iter().map(AgentMessage::role).collect();
    assert_eq!(roles, ["compactionSummary", "assistant"]);
    assert_eq!(context.messages[1], assistant_message("done", 1));
}

#[test]
fn unknown_leaf_falls_back_to_last_entry_and_orphans_stop_walking() {
    let entries = vec![user_entry("1", None, "hello"), assistant_entry("2", Some("1"), "hi")];
    assert_eq!(
        build_session_context(&entries, LeafSelection::Id(&JsString::from("missing")))
            .messages
            .len(),
        2
    );

    let entries = vec![
        user_entry("1", None, "hello"),
        assistant_entry("2", Some("missing"), "orphan"),
    ];
    let context = build_session_context(&entries, LeafSelection::Id(&JsString::from("2")));
    assert_eq!(context.messages.len(), 1);
    assert_eq!(context.messages[0], assistant_message("orphan", 1));
}

#[test]
fn reads_files_and_headers() {
    let dir = temp_dir("session-read");
    let missing = dir.join("nonexistent.jsonl");
    assert!(
        load_entries_from_file(&JsString::from(&missing.display().to_string()))
            .unwrap()
            .is_empty()
    );

    let empty = dir.join("empty.jsonl");
    fs::write(&empty, "").unwrap();
    assert!(
        load_entries_from_file(&JsString::from(&empty.display().to_string()))
            .unwrap()
            .is_empty()
    );

    let no_header = dir.join("no-header.jsonl");
    fs::write(&no_header, "{\"type\":\"message\",\"id\":\"1\"}\n").unwrap();
    assert!(
        load_entries_from_file(&JsString::from(&no_header.display().to_string()))
            .unwrap()
            .is_empty()
    );

    let malformed = dir.join("malformed.jsonl");
    fs::write(&malformed, "not json\n").unwrap();
    assert!(
        load_entries_from_file(&JsString::from(&malformed.display().to_string()))
            .unwrap()
            .is_empty()
    );

    let valid = dir.join("valid.jsonl");
    fs::write(
        &valid,
        "{\"type\":\"session\",\"id\":\"abc\",\"timestamp\":\"2025-01-01T00:00:00Z\",\"cwd\":\"/tmp\"}\n\
         {\"type\":\"message\",\"id\":\"1\",\"parentId\":null,\"timestamp\":\"2025-01-01T00:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"hi\",\"timestamp\":1}}\n",
    )
    .unwrap();
    let entries = load_entries_from_file(&JsString::from(&valid.display().to_string())).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(
        read_session_header(&JsString::from(&valid.display().to_string()))
            .unwrap()
            .id()
            .clone(),
        "abc"
    );

    let mixed = dir.join("mixed.jsonl");
    fs::write(
        &mixed,
        "{\"type\":\"session\",\"id\":\"abc\",\"timestamp\":\"2025-01-01T00:00:00Z\",\"cwd\":\"/tmp\"}\n\
         not valid json\n\
         {\"type\":\"message\",\"id\":\"1\",\"parentId\":null,\"timestamp\":\"2025-01-01T00:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"hi\",\"timestamp\":1}}\n",
    )
    .unwrap();
    assert_eq!(
        load_entries_from_file(&JsString::from(&mixed.display().to_string()))
            .unwrap()
            .len(),
        2
    );

    let header = dir.join("header.jsonl");
    fs::write(
        &header,
        format!(
            "\n  \n{{\"type\":\"session\",\"version\":3,\"id\":\"leading-blank\",\"timestamp\":\"2025-01-01T00:00:00Z\",\"cwd\":\"{}\"}}\n",
            dir.display()
        ),
    )
    .unwrap();
    let manager = SessionManager::open(
        &JsString::from(&header.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    assert_eq!(manager.get_session_id(), "leading-blank");
    assert_eq!(
        *manager.get_cwd(),
        pi_js_compat::node_fs::decode_node_path(std::env::current_dir().unwrap().as_os_str())
    );
}

#[test]
fn finds_most_recent_session_by_mtime_and_cwd() {
    let dir = temp_dir("session-recent");
    assert_eq!(
        find_most_recent_session(&JsString::from(&dir.display().to_string()), None),
        None
    );
    assert_eq!(
        find_most_recent_session(&JsString::from(&dir.join("missing").display().to_string()), None),
        None
    );
    fs::write(dir.join("file.txt"), "hello").unwrap();
    assert_eq!(
        find_most_recent_session(&JsString::from(&dir.display().to_string()), None),
        None
    );

    fs::write(dir.join("invalid.jsonl"), "{\"type\":\"message\"}\n").unwrap();
    assert_eq!(
        find_most_recent_session(&JsString::from(&dir.display().to_string()), None),
        None
    );

    let older = dir.join("older.jsonl");
    write_jsonl(&older, &[header_json("old", "/tmp")]);
    std::thread::sleep(std::time::Duration::from_millis(10));
    let newer = dir.join("newer.jsonl");
    write_jsonl(&newer, &[header_json("new", "/tmp")]);
    assert_eq!(
        find_most_recent_session(&JsString::from(&dir.display().to_string()), None),
        Some(newer.display().to_string()).map(JsString::from)
    );

    let project_a = dir.join("project-a");
    let project_b = dir.join("project-b");
    fs::create_dir_all(&project_a).unwrap();
    fs::create_dir_all(&project_b).unwrap();
    let file_a = dir.join("a.jsonl");
    write_jsonl(&file_a, &[header_json("a", &project_a.display().to_string())]);
    let file_b = dir.join("b.jsonl");
    write_jsonl(&file_b, &[header_json("b", &project_b.display().to_string())]);
    assert_eq!(
        find_most_recent_session(
            &JsString::from(&dir.display().to_string()),
            Some(&JsString::from(&project_a.display().to_string()))
        ),
        Some(file_a.display().to_string()).map(JsString::from)
    );
}

#[test]
fn persists_only_after_the_first_assistant_message() {
    let dir = temp_dir("session-persist");
    let mut session = SessionManager::create(
        &JsString::from(&dir.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        Some(NewSessionOptions {
            id: Some(JsString::from("created-session-id")),
            ..NewSessionOptions::default()
        }),
    )
    .unwrap();
    let file = session.get_session_file().unwrap().to_owned();
    assert!(file.contains("created-session-id"));
    let file_name = Path::new(&encode_node_utf8(&file))
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        file_name.ends_with("_created-session-id.jsonl"),
        "unexpected file name {file_name}"
    );
    assert!(!Path::new(&encode_node_utf8(&file)).exists());

    session.append_message(user_message("hello", 1)).unwrap();
    assert!(!Path::new(&encode_node_utf8(&file)).exists());
    session.append_message(assistant_message("world", 2)).unwrap();
    assert!(Path::new(&encode_node_utf8(&file)).exists());

    let reopened = SessionManager::open(&file, None, None).unwrap();
    assert_eq!(reopened.get_entries().len(), 2);
    assert_eq!(*reopened.get_cwd(), dir.to_str().expect("utf-8 path"));
}

#[test]
fn rewrites_empty_files_and_rejects_non_session_files() {
    let dir = temp_dir("session-corrupt");
    let empty = dir.join("empty.jsonl");
    fs::write(&empty, "").unwrap();
    let manager = SessionManager::open(
        &JsString::from(&empty.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    assert!(!manager.get_session_id().is_empty());
    assert!(manager.get_header().is_some());
    let content = fs::read_to_string(&empty).unwrap();
    assert_eq!(content.lines().count(), 1);
    assert_eq!(
        *manager.get_session_file().unwrap(),
        empty.to_str().expect("utf-8 path")
    );

    let no_header = dir.join("no-header.jsonl");
    let original = "{\"type\":\"message\",\"id\":\"abc\",\"parentId\":\"orphaned\",\"timestamp\":\"2025-01-01T00:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":\"test\"}}\n";
    fs::write(&no_header, original).unwrap();
    let error = SessionManager::open(
        &JsString::from(&no_header.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .err()
    .unwrap();
    assert_eq!(
        error.to_string(),
        format!("Session file is not a valid pi session: {}", no_header.display())
    );
    assert_eq!(fs::read_to_string(&no_header).unwrap(), original);

    let non_session = dir.join("not-a-session.log");
    fs::write(&non_session, "{\"type\":\"event\",\"data\":\"not a session\"}\n").unwrap();
    assert!(SessionManager::open(&JsString::from(&non_session.display().to_string()), None, None).is_err());
}

#[test]
fn loads_active_compacted_slice() {
    let dir = temp_dir("session-active-slice");
    let file = dir.join("compacted.jsonl");
    write_jsonl(
        &file,
        &[
            header_json("session-1", "/tmp"),
            serde_json::json!({"type":"message","id":"old-1","parentId":null,"timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"old 1","timestamp":1}}),
            serde_json::json!({"type":"message","id":"old-2","parentId":"old-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"old 2","timestamp":1}}),
            serde_json::json!({"type":"message","id":"kept-1","parentId":"old-2","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"kept 1","timestamp":1}}),
            serde_json::json!({"type":"message","id":"kept-2","parentId":"kept-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"kept 2","timestamp":1}}),
            serde_json::json!({"type":"compaction","id":"compaction-1","parentId":"kept-2","timestamp":"2025-01-01T00:00:00Z","summary":"summary","firstKeptEntryId":"kept-1","tokensBefore":1000}),
            serde_json::json!({"type":"message","id":"after-1","parentId":"compaction-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"after 1","timestamp":1}}),
            serde_json::json!({"type":"message","id":"after-2","parentId":"after-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"after 2","timestamp":1}}),
        ],
    );
    let session = SessionManager::open(
        &JsString::from(&file.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    assert_eq!(
        ids(&session.get_entries()),
        ["kept-1", "kept-2", "compaction-1", "after-1", "after-2"]
    );
}

#[test]
fn summarized_jsonl_session_info_is_not_authoritative() {
    let dir = temp_dir("session-name-slice");
    let file = dir.join("named-compacted.jsonl");
    write_jsonl(
        &file,
        &[
            header_json("session-1", "/tmp"),
            serde_json::json!({"type":"message","id":"old-1","parentId":null,"timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"old","timestamp":1}}),
            serde_json::json!({"type":"session_info","id":"name-old","parentId":"old-1","timestamp":"2025-01-01T00:00:01Z","name":"Old Name"}),
            serde_json::json!({"type":"session_info","id":"name-new","parentId":"name-old","timestamp":"2025-01-01T00:00:02Z","name":"Current Name"}),
            serde_json::json!({"type":"message","id":"kept-1","parentId":"name-new","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"kept","timestamp":1}}),
            serde_json::json!({"type":"compaction","id":"compaction-1","parentId":"kept-1","timestamp":"2025-01-01T00:00:03Z","summary":"summary","firstKeptEntryId":"kept-1","tokensBefore":1000}),
            serde_json::json!({"type":"message","id":"after-1","parentId":"compaction-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"after","timestamp":1}}),
        ],
    );
    let session = SessionManager::open(
        &JsString::from(&file.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    assert_eq!(session.get_session_name(), None);
    assert!(!session.has_session_name_state());
    assert_eq!(ids(&session.get_entries()), ["kept-1", "compaction-1", "after-1"]);
}

#[test]
fn restores_the_sqlite_session_name_without_loading_pre_cutoff_session_info() {
    let dir = temp_dir("session-name-sqlite");
    let file = dir.join("sqlite-named-compacted.jsonl");
    let control_db_path = dir.join("control.sqlite").display().to_string();
    let initial_cwd = dir.join("initial").display().to_string();
    let relocated_cwd = dir.join("relocated").display().to_string();
    write_jsonl(
        &file,
        &[
            header_json("session-1", &initial_cwd),
            serde_json::json!({"type":"message","id":"old-1","parentId":null,"timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"old","timestamp":1}}),
            serde_json::json!({
                "type":"custom_message",
                "id":"cwd-change",
                "parentId":"old-1",
                "timestamp":"2025-01-01T00:00:01Z",
                "customType":"cwd_changed",
                "content":format!("Working directory changed to {relocated_cwd}."),
                "details":{"previousCwd":initial_cwd,"cwd":relocated_cwd},
                "display":true
            }),
            serde_json::json!({"type":"message","id":"kept-1","parentId":"cwd-change","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"kept","timestamp":1}}),
            serde_json::json!({"type":"compaction","id":"compaction-1","parentId":"kept-1","timestamp":"2025-01-01T00:00:02Z","summary":"summary","firstKeptEntryId":"kept-1","tokensBefore":1000}),
            serde_json::json!({"type":"message","id":"after-1","parentId":"compaction-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"after","timestamp":1}}),
        ],
    );
    let session_path = file.display().to_string();
    let mut metadata = metadata_row(&session_path, "session-1", &relocated_cwd, Some("SQLite Session Name"));
    metadata.message_count = 3;
    write_session_metadata(&control_db_path, &metadata).expect("metadata");

    let mut session = SessionManager::open(
        &JsString::from(&session_path),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    session
        .set_metadata_control_db_path(
            Some(&JsString::from(&control_db_path)),
            SetMetadataControlDbPathOptions::default(),
        )
        .expect("attach control db");

    assert!(
        !session
            .get_entries()
            .iter()
            .any(|entry| entry.entry_type() == "session_info")
    );
    assert_eq!(*session.get_cwd(), relocated_cwd);
    assert_eq!(
        session.get_session_name().as_ref(),
        Some(&JsString::from("SQLite Session Name"))
    );
    assert!(session.has_session_name_state());
}

#[test]
fn restores_an_explicit_name_clear_without_making_the_session_autoname_eligible() {
    let dir = temp_dir("session-name-clear");
    let control_db_path = dir.join("control.sqlite").display().to_string();
    let file = dir.join("cleared.jsonl");
    write_jsonl(&file, &[header_json("cleared", &dir.display().to_string())]);
    let session_path = file.display().to_string();
    write_session_metadata(
        &control_db_path,
        &metadata_row(&session_path, "cleared", &dir.display().to_string(), None),
    )
    .expect("metadata");
    write_session_name(&control_db_path, &session_path, None).expect("explicit clear");

    let mut session = SessionManager::open(
        &JsString::from(&session_path),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    session
        .set_metadata_control_db_path(
            Some(&JsString::from(&control_db_path)),
            SetMetadataControlDbPathOptions::default(),
        )
        .expect("attach control db");

    assert_eq!(session.get_session_name(), None);
    assert!(session.has_session_name_state());
    let state = read_session_name_state(&control_db_path, &session_path).expect("state");
    assert_eq!(state.name, None);
    assert!(state.has_stored_value);
}

#[test]
fn rehydrates_the_sqlite_session_name_when_switching_files() {
    let dir = temp_dir("session-name-switch");
    let control_db_path = dir.join("control.sqlite").display().to_string();
    let cwd = dir.display().to_string();
    let first = dir.join("first.jsonl");
    let second = dir.join("second.jsonl");
    for (file, id, name) in [(&first, "first", "First Name"), (&second, "second", "Second Name")] {
        write_jsonl(file, &[header_json(id, &cwd)]);
        write_session_metadata(
            &control_db_path,
            &metadata_row(&file.display().to_string(), id, &cwd, Some(name)),
        )
        .expect("metadata");
    }

    let mut session = SessionManager::open(
        &JsString::from(&first.display().to_string()),
        Some(&JsString::from(&cwd)),
        None,
    )
    .unwrap();
    session
        .set_metadata_control_db_path(
            Some(&JsString::from(&control_db_path)),
            SetMetadataControlDbPathOptions::default(),
        )
        .expect("attach control db");
    assert_eq!(session.get_session_name().as_ref(), Some(&JsString::from("First Name")));

    session
        .set_session_file(&JsString::from(&second.display().to_string()))
        .expect("switch");
    assert_eq!(
        session.get_session_name().as_ref(),
        Some(&JsString::from("Second Name"))
    );
}

#[test]
fn does_not_carry_a_session_name_into_a_branched_session() {
    let mut session = SessionManager::in_memory(Some(&JsString::from("/tmp")), None).unwrap();
    let leaf_id = session.append_message(user_message("branch me", 1)).unwrap();
    session
        .set_session_name(Some(&JsString::from("Parent Name")))
        .expect("name");

    session.create_branched_session(&leaf_id).expect("branch");

    assert_eq!(session.get_session_name(), None);
    assert!(!session.has_session_name_state());
}

fn metadata_row(session_path: &str, id: &str, cwd: &str, name: Option<&str>) -> WritableSessionMetadata {
    WritableSessionMetadata {
        session_path: JsString::from(session_path.to_owned()),
        id: JsString::from(id.to_owned()),
        cwd: JsString::from(cwd.to_owned()),
        name: name.map(str::to_owned).map(JsString::from),
        parent_session_path: None,
        archived_at: None,
        is_archived: None,
        is_subagent: None,
        subagent_name: None,
        model_provider: None,
        model_id: None,
        thinking_level: None,
        created_at: JsString::from("2025-01-01T00:00:00.000Z"),
        modified_at: JsString::from("2025-01-01T00:00:03.000Z"),
        message_count: 0,
        first_message: JsString::from("(no messages)"),
    }
}

#[test]
fn active_slice_uses_latest_compaction() {
    let dir = temp_dir("session-latest-compaction");
    let file = dir.join("multiple-compactions.jsonl");
    write_jsonl(
        &file,
        &[
            header_json("session-1", "/tmp"),
            serde_json::json!({"type":"message","id":"old-1","parentId":null,"timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"old","timestamp":1}}),
            serde_json::json!({"type":"message","id":"first-kept","parentId":"old-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"first kept","timestamp":1}}),
            serde_json::json!({"type":"compaction","id":"compaction-1","parentId":"first-kept","timestamp":"2025-01-01T00:00:00Z","summary":"first summary","firstKeptEntryId":"first-kept","tokensBefore":500}),
            serde_json::json!({"type":"message","id":"between","parentId":"compaction-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"between","timestamp":1}}),
            serde_json::json!({"type":"message","id":"latest-kept","parentId":"between","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"latest kept","timestamp":1}}),
            serde_json::json!({"type":"compaction","id":"compaction-2","parentId":"latest-kept","timestamp":"2025-01-01T00:01:00Z","summary":"latest summary","firstKeptEntryId":"latest-kept","tokensBefore":1000}),
            serde_json::json!({"type":"message","id":"after-1","parentId":"compaction-2","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"after","timestamp":1}}),
        ],
    );
    let session = SessionManager::open(
        &JsString::from(&file.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    assert_eq!(ids(&session.get_entries()), ["latest-kept", "compaction-2", "after-1"]);
}

#[test]
fn active_slice_errors_and_truncated_tail() {
    let dir = temp_dir("session-slice-errors");
    let broken = dir.join("broken-parent.jsonl");
    write_jsonl(
        &broken,
        &[
            header_json("session-1", "/tmp"),
            serde_json::json!({"type":"message","id":"leaf","parentId":"missing-parent","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"orphan","timestamp":1}}),
        ],
    );
    let error = SessionManager::open(
        &JsString::from(&broken.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("broken active parent chain"));

    let missing_first = dir.join("missing-first.jsonl");
    write_jsonl(
        &missing_first,
        &[
            header_json("session-1", "/tmp"),
            serde_json::json!({"type":"message","id":"old-1","parentId":null,"timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"old","timestamp":1}}),
            serde_json::json!({"type":"compaction","id":"compaction-1","parentId":"old-1","timestamp":"2025-01-01T00:00:00Z","summary":"summary","firstKeptEntryId":"missing-entry","tokensBefore":1000}),
            serde_json::json!({"type":"message","id":"after-1","parentId":"compaction-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"after","timestamp":1}}),
        ],
    );
    let error = SessionManager::open(
        &JsString::from(&missing_first.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("missing firstKeptEntryId"));

    let malformed = dir.join("malformed-interior.jsonl");
    write_jsonl(
        &malformed,
        &[
            header_json("session-1", "/tmp"),
            serde_json::json!({"type":"message","id":"kept-1","parentId":null,"timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"kept","timestamp":1}}),
            serde_json::json!({"type":"compaction","id":"compaction-1","parentId":"kept-1","timestamp":"2025-01-01T00:00:00Z","summary":"summary","firstKeptEntryId":"kept-1","tokensBefore":1000}),
            serde_json::json!({"type":"message","id":"after-1","parentId":"compaction-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"after","timestamp":1}}),
        ],
    );
    let content = fs::read_to_string(&malformed).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let mut broken_lines = lines.clone();
    broken_lines.insert(2, "not json");
    fs::write(&malformed, format!("{}\n", broken_lines.join("\n"))).unwrap();
    let error = SessionManager::open(
        &JsString::from(&malformed.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("malformed JSONL entry"));

    let truncated = dir.join("truncated-tail.jsonl");
    write_jsonl(
        &truncated,
        &[
            header_json("session-1", "/tmp"),
            serde_json::json!({"type":"message","id":"kept-1","parentId":null,"timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"kept","timestamp":1}}),
            serde_json::json!({"type":"compaction","id":"compaction-1","parentId":"kept-1","timestamp":"2025-01-01T00:00:00Z","summary":"summary","firstKeptEntryId":"kept-1","tokensBefore":1000}),
            serde_json::json!({"type":"message","id":"after-1","parentId":"compaction-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"after","timestamp":1}}),
        ],
    );
    let mut content = fs::read_to_string(&truncated).unwrap();
    content.push_str("{\"type\":\"message\"");
    fs::write(&truncated, content).unwrap();
    let session = SessionManager::open(
        &JsString::from(&truncated.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    assert_eq!(ids(&session.get_entries()), ["kept-1", "compaction-1", "after-1"]);
}

#[test]
fn restores_cwd_from_summarized_prefix() {
    let dir = temp_dir("session-cwd-prefix");
    let initial = dir.join("initial");
    let relocated = dir.join("relocated");
    let file = dir.join("compacted-cwd.jsonl");
    write_jsonl(
        &file,
        &[
            header_json("session-1", &initial.display().to_string()),
            serde_json::json!({"type":"message","id":"old-1","parentId":null,"timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"old","timestamp":1}}),
            serde_json::json!({"type":"custom_message","id":"cwd-change","parentId":"old-1","timestamp":"2025-01-01T00:00:00Z","customType":"cwd_changed","content":format!("Working directory changed to {}.", relocated.display()),"details":{"previousCwd":initial.display().to_string(),"cwd":relocated.display().to_string()},"display":true}),
            serde_json::json!({"type":"message","id":"kept-1","parentId":"cwd-change","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"kept","timestamp":1}}),
            serde_json::json!({"type":"compaction","id":"compaction-1","parentId":"kept-1","timestamp":"2025-01-01T00:00:00Z","summary":"summary","firstKeptEntryId":"kept-1","tokensBefore":1000}),
            serde_json::json!({"type":"message","id":"after-1","parentId":"compaction-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"after","timestamp":1}}),
        ],
    );
    let session = SessionManager::open(
        &JsString::from(&file.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    assert_eq!(*session.get_cwd(), relocated.to_str().expect("utf-8 path"));
    assert_eq!(ids(&session.get_entries()), ["kept-1", "compaction-1", "after-1"]);
}

#[test]
fn clears_labels_omitted_before_retained_slice() {
    let dir = temp_dir("session-labels-slice");
    let file = dir.join("compacted-label.jsonl");
    write_jsonl(
        &file,
        &[
            header_json("compacted-session", "/tmp"),
            serde_json::json!({"type":"message","id":"target","parentId":null,"timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"target","timestamp":1}}),
            serde_json::json!({"type":"label","id":"label-1","parentId":"target","timestamp":"2025-01-01T00:00:00Z","targetId":"target","label":"stale"}),
            serde_json::json!({"type":"message","id":"kept-1","parentId":"label-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"kept","timestamp":1}}),
            serde_json::json!({"type":"compaction","id":"compaction-1","parentId":"kept-1","timestamp":"2025-01-01T00:00:00Z","summary":"summary","firstKeptEntryId":"kept-1","tokensBefore":1000}),
            serde_json::json!({"type":"message","id":"after-1","parentId":"compaction-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"after","timestamp":1}}),
        ],
    );
    let session = SessionManager::open(
        &JsString::from(&file.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    assert_eq!(session.get_label(&JsString::from("target")), None);
}

#[test]
fn relocation_preserves_the_complete_jsonl() {
    let dir = temp_dir("session-relocate");
    let project_a = dir.join("project-a");
    let project_b = dir.join("project-b");
    let agent_dir = dir.join("agent");
    fs::create_dir_all(&project_a).unwrap();
    fs::create_dir_all(&project_b).unwrap();
    let source_dir = get_default_session_dir(
        &JsString::from(&project_a.display().to_string()),
        Some(&JsString::from(&agent_dir.display().to_string())),
    )
    .unwrap();
    let source_file = Path::new(&encode_node_utf8(&source_dir)).join("compacted.jsonl");
    let entries = vec![
        serde_json::json!({"type":"session","version":3,"id":"session-1","timestamp":"2026-08-04T00:00:00.000Z","cwd":project_a.display().to_string()}),
        serde_json::json!({"type":"message","id":"old-1","parentId":null,"timestamp":"2026-08-04T00:00:00.000Z","message":{"role":"user","content":"summarized","timestamp":1}}),
        serde_json::json!({"type":"message","id":"kept-1","parentId":"old-1","timestamp":"2026-08-04T00:00:00.000Z","message":{"role":"user","content":"kept","timestamp":1}}),
        serde_json::json!({"type":"compaction","id":"compaction-1","parentId":"kept-1","timestamp":"2026-08-04T00:00:03.000Z","summary":"summary","firstKeptEntryId":"kept-1","tokensBefore":1000}),
        serde_json::json!({"type":"message","id":"after-1","parentId":"compaction-1","timestamp":"2026-08-04T00:00:00.000Z","message":{"role":"user","content":"after","timestamp":1}}),
    ];
    write_jsonl(&source_file, &entries);
    let original = fs::read_to_string(&source_file).unwrap();

    let mut session = SessionManager::open(
        &JsString::from(&source_file.display().to_string()),
        Some(&source_dir),
        None,
    )
    .unwrap();
    assert_eq!(ids(&session.get_entries()), ["kept-1", "compaction-1", "after-1"]);
    session
        .relocate(
            &JsString::from(&project_b.display().to_string()),
            Some(&JsString::from(&agent_dir.display().to_string())),
        )
        .unwrap();

    let moved = session.get_session_file().unwrap().to_owned();
    let expected_dir = get_default_session_dir(
        &JsString::from(&project_b.display().to_string()),
        Some(&JsString::from(&agent_dir.display().to_string())),
    )
    .unwrap();
    assert_eq!(
        moved,
        Path::new(&encode_node_utf8(&expected_dir))
            .join("compacted.jsonl")
            .display()
            .to_string()
    );
    assert!(!source_file.exists());
    assert_eq!(fs::read_to_string(&encode_node_utf8(&moved)).unwrap(), original);
    assert_eq!(
        ids(&SessionManager::open(&moved, None, None).unwrap().get_entries()),
        ["kept-1", "compaction-1", "after-1"]
    );
}

#[test]
fn forks_only_the_active_compacted_slice() {
    let dir = temp_dir("session-fork-slice");
    let file = dir.join("fork-source.jsonl");
    write_jsonl(
        &file,
        &[
            header_json("session-1", &dir.display().to_string()),
            serde_json::json!({"type":"message","id":"old-1","parentId":null,"timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"summarized","timestamp":1}}),
            serde_json::json!({"type":"message","id":"kept-1","parentId":"old-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"kept","timestamp":1}}),
            serde_json::json!({"type":"compaction","id":"compaction-1","parentId":"kept-1","timestamp":"2025-01-01T00:00:00Z","summary":"summary","firstKeptEntryId":"kept-1","tokensBefore":1000}),
            serde_json::json!({"type":"message","id":"after-1","parentId":"compaction-1","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"after","timestamp":1}}),
        ],
    );
    let forks = dir.join("forks");
    let forked = SessionManager::fork_from(
        &JsString::from(&file.display().to_string()),
        &JsString::from(&dir.display().to_string()),
        Some(&JsString::from(&forks.display().to_string())),
        Some(NewSessionOptions {
            id: Some(JsString::from("forked")),
            ..NewSessionOptions::default()
        }),
        None,
    )
    .unwrap();
    let forked_file = forked.get_session_file().unwrap().to_owned();
    let serialized_ids: Vec<String> = fs::read_to_string(&encode_node_utf8(&forked_file))
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|entry| entry.get("type").and_then(|value| value.as_str()) != Some("session"))
        .map(|entry| entry.get("id").unwrap().as_str().unwrap().to_owned())
        .collect();
    assert_eq!(serialized_ids, ["kept-1", "compaction-1", "after-1"]);
    assert_eq!(
        forked.get_header().unwrap().parent_session(),
        Some(&JsString::from(file.display().to_string()))
    );
}

#[test]
fn fork_selection_all_none_and_from() {
    let dir = temp_dir("session-fork-selection");
    let source = dir.join("source.jsonl");
    write_jsonl(
        &source,
        &[
            header_json("source", &dir.display().to_string()),
            serde_json::json!({"type":"message","id":"a","parentId":null,"timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":"a","timestamp":1}}),
            serde_json::json!({"type":"message","id":"b","parentId":"a","timestamp":"2025-01-01T00:00:00Z","message":{"role":"assistant","content":[{"type":"text","text":"b"}],"api":"x","provider":"x","model":"x","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":2}}),
        ],
    );
    let all = SessionManager::fork_from(
        &JsString::from(&source.display().to_string()),
        &JsString::from(&dir.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        Some(NewSessionOptions {
            id: Some(JsString::from("all")),
            ..NewSessionOptions::default()
        }),
        None,
    )
    .unwrap();
    assert_eq!(all.get_entries().len(), 2);
    let none = SessionManager::fork_from(
        &JsString::from(&source.display().to_string()),
        &JsString::from(&dir.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        Some(NewSessionOptions {
            id: Some(JsString::from("none")),
            ..NewSessionOptions::default()
        }),
        Some(ForkEntrySelection::None),
    )
    .unwrap();
    assert!(none.get_entries().is_empty());
    let from = SessionManager::fork_from(
        &JsString::from(&source.display().to_string()),
        &JsString::from(&dir.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        Some(NewSessionOptions {
            id: Some(JsString::from("from")),
            ..NewSessionOptions::default()
        }),
        Some(ForkEntrySelection::From(JsString::from("a"))),
    )
    .unwrap();
    assert_eq!(ids(&from.get_entries()), ["a"]);
}

#[test]
fn tree_traversal_and_branching() {
    let mut session = SessionManager::in_memory(None, None).unwrap();
    assert_eq!(session.get_leaf_id(), None);
    let id1 = session.append_message(user_message("1", 1)).unwrap();
    let id2 = session.append_message(assistant_message("2", 2)).unwrap();
    let id3 = session.append_message(user_message("3", 3)).unwrap();
    assert_eq!(session.get_leaf_id(), Some(&id3));
    assert_eq!(ids(&session.get_branch(None)), [id1.clone(), id2.clone(), id3.clone()]);
    assert_eq!(ids(&session.get_branch(Some(&id2))), [id1.clone(), id2.clone()]);

    session.branch(&id2).unwrap();
    let id4 = session.append_message(user_message("4-branch", 4)).unwrap();
    let tree = session.get_tree();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].entry.id(), Some(&id1));
    assert_eq!(tree[0].children.len(), 1);
    assert_eq!(tree[0].children[0].entry.id(), Some(&id2));
    assert_eq!(tree[0].children[0].children.len(), 2);
    let child_ids: Vec<&JsString> = tree[0].children[0]
        .children
        .iter()
        .map(|node| node.entry.id().unwrap())
        .collect();
    assert!(child_ids.contains(&&id3));
    assert!(child_ids.contains(&&id4));

    assert_eq!(session.get_leaf_entry().unwrap().id(), Some(&id4));
    assert_eq!(session.get_entry(&id1).unwrap().id(), Some(&id1));
    assert!(session.branch(&JsString::from("nonexistent")).is_err());
}

#[test]
fn labels_and_tree_nodes() {
    let mut session = SessionManager::in_memory(None, None).unwrap();
    let msg1 = session.append_message(user_message("hello", 1)).unwrap();
    assert_eq!(session.get_label(&msg1), None);
    let label_id = session
        .append_label_change(&msg1, Some(&JsString::from("checkpoint")))
        .unwrap();
    assert_eq!(session.get_label(&msg1), Some(&JsString::from("checkpoint")));
    let label_entry = session.get_entry(&label_id).unwrap().as_label().unwrap().clone();
    assert_eq!(label_entry.target_id, msg1);

    session.append_label_change(&msg1, None).unwrap();
    assert_eq!(session.get_label(&msg1), None);
    session
        .append_label_change(&msg1, Some(&JsString::from("first")))
        .unwrap();
    let last_label = session
        .append_label_change(&msg1, Some(&JsString::from("third")))
        .unwrap();
    assert_eq!(session.get_label(&msg1), Some(&JsString::from("third")));
    let tree = session.get_tree();
    let node = tree.iter().find(|node| node.entry.id() == Some(&msg1)).unwrap();
    let last = session.get_entry(&last_label).unwrap().as_label().unwrap();
    assert_eq!(node.label.as_ref(), Some(&JsString::from("third")));
    assert_eq!(node.label_timestamp.as_ref(), Some(&last.timestamp));

    assert_eq!(
        session
            .append_label_change(&JsString::from("non-existent"), Some(&JsString::from("label")))
            .unwrap_err()
            .to_string(),
        "Entry non-existent not found"
    );
}

#[test]
fn branched_session_rewires_labels_and_preserves_path() {
    let mut session = SessionManager::in_memory(None, None).unwrap();
    let id1 = session.append_message(user_message("1", 1)).unwrap();
    session
        .append_label_change(&id1, Some(&JsString::from("checkpoint")))
        .unwrap();
    let model_id = session
        .append_model_change(&JsString::from("anthropic"), &JsString::from("claude-test"))
        .unwrap();
    let id2 = session.append_message(user_message("followup", 2)).unwrap();
    session.create_branched_session(&id2).unwrap();
    assert_eq!(session.get_entry(&model_id).unwrap().parent_id(), Some(&id1));
    assert_eq!(session.get_label(&id1), Some(&JsString::from("checkpoint")));
}

#[test]
fn branched_session_does_not_duplicate_entries() {
    let dir = temp_dir("session-branch-dedup");
    let mut session = SessionManager::create(
        &JsString::from(&dir.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    let id1 = session.append_message(user_message("first question", 1)).unwrap();
    session.append_message(assistant_message("first answer", 2)).unwrap();
    session.append_message(user_message("second question", 3)).unwrap();
    session.append_message(assistant_message("second answer", 4)).unwrap();
    let new_file = session.create_branched_session(&id1).unwrap().unwrap();
    assert!(!Path::new(&encode_node_utf8(&new_file)).exists());
    session
        .append_custom_entry(
            &JsString::from("preset-state"),
            Some(JsValue::from(serde_json::json!({"name":"plan"}))),
        )
        .unwrap();
    session.append_message(assistant_message("new answer", 5)).unwrap();
    assert!(Path::new(&encode_node_utf8(&new_file)).exists());
    let records: Vec<serde_json::Value> = fs::read_to_string(&encode_node_utf8(&new_file))
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.iter().filter(|record| record["type"] == "session").count(), 1);
    let entry_ids: Vec<&str> = records
        .iter()
        .filter(|record| record["type"] != "session")
        .filter_map(|record| record["id"].as_str())
        .collect();
    let unique: std::collections::HashSet<&str> = entry_ids.iter().copied().collect();
    assert_eq!(unique.len(), entry_ids.len());
}

#[test]
fn update_compaction_summary_rewrites_and_restores_on_failure() {
    let dir = temp_dir("session-compaction-edit");
    let mut session = SessionManager::create(
        &JsString::from(&dir.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    session.append_message(user_message("old question", 1)).unwrap();
    session.append_message(assistant_message("old answer", 2)).unwrap();
    let first_kept = session.append_message(user_message("kept question", 3)).unwrap();
    session.append_message(assistant_message("kept answer", 4)).unwrap();
    let compaction_id = session
        .append_compaction(
            &JsString::from("original summary"),
            &first_kept,
            42_000.0,
            pi_coding_agent::core::session_manager::CompactionAppendOptions {
                details: Some(JsValue::from(serde_json::json!({"source":"test"}))),
                from_hook: Some(true),
                duration_ms: Some(321.0),
                ..Default::default()
            },
        )
        .unwrap();
    session.append_message(user_message("new question", 5)).unwrap();
    session.append_message(assistant_message("new answer", 6)).unwrap();
    let file = session.get_session_file().unwrap().to_owned();
    let leaf = session.get_leaf_id().unwrap().to_owned();

    session
        .update_compaction_summary(&compaction_id, &JsString::from("edited summary"))
        .unwrap();
    let reopened = SessionManager::open(&file, None, None).unwrap();
    let compaction = reopened.get_entry(&compaction_id).unwrap().as_compaction().unwrap();
    assert_eq!(compaction.summary, "edited summary");
    assert_eq!(
        compaction.details.as_deref(),
        Some(&JsValue::from(serde_json::json!({"source":"test"})))
    );
    assert_eq!(reopened.get_leaf_id(), Some(&leaf));
    assert_eq!(reopened.build_session_context().messages[0].role(), "compactionSummary");

    let provider_native = pi_ai::types::ProviderNativeContext {
        provider: "openai".to_owned(),
        api: "openai-responses".to_owned(),
        format: pi_ai::types::ProviderNativeFormat::OpenAIResponsesInput,
        value: JsValue::from(
            serde_json::json!([{"type":"compaction_summary","encrypted_content":"encrypted-native-checkpoint"}]),
        ),
    };
    let id = session
        .append_compaction(
            &JsString::from("native summary"),
            &first_kept,
            42_000.0,
            pi_coding_agent::core::session_manager::CompactionAppendOptions {
                details: Some(JsValue::from(serde_json::json!({"type":"openai-remote-compaction"}))),
                from_hook: Some(true),
                duration_ms: Some(321.0),
                provider_native: Some(provider_native),
                ..Default::default()
            },
        )
        .unwrap();
    session
        .update_compaction_summary(&id, &JsString::from("edited plaintext summary"))
        .unwrap();
    let reopened = SessionManager::open(&file, None, None).unwrap();
    let edited = reopened.get_entry(&id).unwrap().as_compaction().unwrap();
    assert_eq!(edited.summary, "edited plaintext summary");
    assert_eq!(edited.details, None);
    assert_eq!(edited.provider_native, None);
    let messages = reopened.build_session_context().messages;
    assert_eq!(messages[0].role(), "compactionSummary");
    assert!(
        !serde_json::to_string(&messages[0])
            .unwrap()
            .contains("encrypted-native-checkpoint")
    );
}

#[test]
fn update_compaction_summary_restores_when_persistence_fails() {
    let dir = temp_dir("session-compaction-fail");
    let mut session = SessionManager::create(
        &JsString::from(&dir.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    let first_kept = session.append_message(user_message("kept", 1)).unwrap();
    session.append_message(assistant_message("answer", 2)).unwrap();
    let id = session
        .append_compaction(
            &JsString::from("original summary"),
            &first_kept,
            42_000.0,
            pi_coding_agent::core::session_manager::CompactionAppendOptions {
                from_hook: Some(true),
                duration_ms: Some(321.0),
                ..Default::default()
            },
        )
        .unwrap();
    let file = session.get_session_file().unwrap().to_owned();
    fs::remove_file(&encode_node_utf8(&file)).unwrap();
    fs::create_dir(&encode_node_utf8(&file)).unwrap();
    assert!(
        session
            .update_compaction_summary(&id, &JsString::from("edited summary"))
            .is_err()
    );
    let compaction = session.get_entry(&id).unwrap().as_compaction().unwrap();
    assert_eq!(compaction.summary, "original summary");
}

#[test]
fn custom_entries_participate_in_the_tree_but_not_context() {
    let mut session = SessionManager::in_memory(None, None).unwrap();
    let msg1 = session.append_message(user_message("hello", 1)).unwrap();
    let custom = session
        .append_custom_entry(
            &JsString::from("my_data"),
            Some(JsValue::from(serde_json::json!({"foo":"bar"}))),
        )
        .unwrap();
    let msg2 = session.append_message(assistant_message("hi", 2)).unwrap();
    let entries = session.get_entries();
    assert_eq!(entries.len(), 3);
    let custom_entry = session.get_entry(&custom).unwrap();
    match custom_entry {
        SessionEntry::Known(KnownSessionEntry::Custom(entry)) => {
            assert_eq!(entry.custom_type, "my_data");
            assert_eq!(entry.data, (Some(serde_json::json!({"foo":"bar"}))).map(JsValue::from));
            assert_eq!(entry.parent_id.as_ref(), Some(&msg1));
        }
        other => panic!("unexpected entry {other:?}"),
    }
    assert_eq!(ids(&session.get_branch(None)), [msg1, custom, msg2]);
    assert_eq!(session.build_session_context().messages.len(), 2);
}

#[test]
fn session_info_listing_uses_message_timestamps_and_skips_extension_prompts() {
    let dir = temp_dir("session-info");
    let mut session = SessionManager::create(
        &JsString::from("/tmp"),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    session
        .append_message(AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(JsString::from("coordination update")),
            input_source: Some(pi_ai::types::InputSource::Extension),
            provider_native: None,
            timestamp: 1.0,
        }))
        .unwrap();
    session
        .append_message(assistant_message("coordination acknowledged", 2))
        .unwrap();
    session
        .append_message(user_message("resume the implementation", 3))
        .unwrap();
    session
        .append_message(assistant_message("implementation resumed", 1_700_000_000_000))
        .unwrap();
    let sessions = SessionManager::list(
        &JsString::from("/tmp"),
        Some(&JsString::from(&dir.display().to_string())),
        None,
        None,
    )
    .unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].first_message, "resume the implementation");
    assert_eq!(sessions[0].modified, 1_700_000_000_000.0);
    assert_eq!(sessions[0].message_count, 4);
}

#[test]
fn lists_sessions_scoped_by_cwd() {
    let dir = temp_dir("session-list");
    let project_a = dir.join("project-a");
    let project_b = dir.join("project-b");
    fs::create_dir_all(&project_a).unwrap();
    fs::create_dir_all(&project_b).unwrap();
    for (project, label) in [(&project_a, "from A"), (&project_b, "from B")] {
        let mut session = SessionManager::create(
            &JsString::from(&project.display().to_string()),
            Some(&JsString::from(&dir.display().to_string())),
            None,
        )
        .unwrap();
        session.append_message(user_message(label, 1)).unwrap();
        session
            .append_message(assistant_message(&format!("reply to {label}"), 2))
            .unwrap();
    }
    let current = SessionManager::list(
        &JsString::from(&project_a.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
        None,
    )
    .unwrap();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].cwd, project_a.display().to_string());
    let all = SessionManager::list_all(Some(&JsString::from(&dir.display().to_string())), None, None).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn default_session_dir_encodes_cwd() {
    let dir = temp_dir("session-default-dir");
    let cwd = dir.join("project");
    let agent = dir.join("agent");
    fs::create_dir_all(&cwd).unwrap();
    let expected = format!(
        "--{}--",
        cwd.display()
            .to_string()
            .trim_start_matches('/')
            .replace(['/', '\\', ':'], "-")
    );
    assert_eq!(
        get_default_session_dir_path(
            &JsString::from(&cwd.display().to_string()),
            &JsString::from(&agent.display().to_string())
        )
        .unwrap(),
        agent.join("sessions").join(expected).display().to_string()
    );
    let created = get_default_session_dir(
        &JsString::from(&cwd.display().to_string()),
        Some(&JsString::from(&agent.display().to_string())),
    )
    .unwrap();
    assert!(Path::new(&encode_node_utf8(&created)).exists());
}

#[test]
fn continues_most_recent_session() {
    let dir = temp_dir("session-continue");
    let project = dir.join("project");
    fs::create_dir_all(&project).unwrap();
    let mut session = SessionManager::create(
        &JsString::from(&project.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    session.append_message(user_message("hello", 1)).unwrap();
    session.append_message(assistant_message("hi", 2)).unwrap();
    let file = session.get_session_file().unwrap().to_owned();
    let continued = SessionManager::continue_recent(
        &JsString::from(&project.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
    )
    .unwrap();
    assert_eq!(continued.get_session_file(), Some(&file));
}

#[test]
fn tree_entry_ids_helper_matches_tree_order() {
    let mut session = SessionManager::in_memory(None, None).unwrap();
    let id1 = session.append_message(user_message("1", 1)).unwrap();
    let id2 = session.append_message(assistant_message("2", 2)).unwrap();
    assert_eq!(tree_ids(&session.get_tree()), [id1, id2]);
}

#[test]
fn control_db_metadata_persists_model_and_thinking_level() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().display().to_string();
    let sessions = dir.path().join("sessions").display().to_string();
    let control_db = dir.path().join("control.db").display().to_string();
    let mut session = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&sessions)), None).unwrap();
    session
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    session
        .append_message(user_message("hello", 1_700_000_000_000))
        .unwrap();

    session
        .set_session_model(&JsString::from("faux"), &JsString::from("faux-1"))
        .expect("set model");
    session
        .set_session_thinking_level(&JsString::from("high"))
        .expect("set thinking level");

    let settings = session
        .read_persisted_session_settings()
        .expect("read settings")
        .expect("persisted settings");
    let model = settings.model.expect("persisted model");
    assert_eq!(model.provider, "faux");
    assert_eq!(model.model_id, "faux-1");
    assert_eq!(settings.thinking_level.as_ref(), Some(&JsString::from("high")));
}

#[test]
fn custom_session_ids_round_trip() {
    let mut session = SessionManager::in_memory(None, None).unwrap();
    session
        .new_session(Some(NewSessionOptions {
            id: Some(JsString::from("abc-123_def.456")),
            ..NewSessionOptions::default()
        }))
        .unwrap();
    assert_eq!(session.get_session_id(), "abc-123_def.456");
    assert_eq!(session.get_header().unwrap().id(), "abc-123_def.456");
    assert_eq!(session.get_header().unwrap().version(), Some(CURRENT_SESSION_VERSION));
}

#[test]
fn control_db_metadata_errors_propagate() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().display().to_string();
    let sessions = dir.path().join("sessions").display().to_string();
    let broken_db = dir.path().join("not-a-db").display().to_string();
    fs::create_dir_all(&broken_db).unwrap();
    let mut session = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&sessions)), None).unwrap();
    session
        .set_metadata_control_db_path(Some(&JsString::from(&broken_db)), Default::default())
        .expect_err("restore from an unreadable control db throws");
    let session_file = session.get_session_file().unwrap().to_owned();

    assert!(session.get_session_goal_json().is_err());
    assert!(session.get_session_goal_json_for_session(&session_file).is_err());
    assert!(session.set_session_goal_json(&JsString::from("{}")).is_err());
    assert!(session.clear_session_goal_json().is_err());
    assert!(session.read_persisted_session_settings().is_err());
    assert!(
        session
            .set_session_model(&JsString::from("faux"), &JsString::from("faux-1"))
            .is_err()
    );
    assert!(session.set_session_thinking_level(&JsString::from("high")).is_err());
    assert!(
        session
            .append_message(user_message("hello", 1_700_000_000_000))
            .is_err(),
        "the post-append metadata snapshot write throws"
    );
}

#[test]
fn metadata_empty_paths_and_model_fields_use_js_truthiness() {
    let mut session = SessionManager::in_memory(Some(&JsString::from("/tmp")), None).unwrap();
    session
        .set_metadata_control_db_path(Some(&JsString::from("")), Default::default())
        .expect("empty control db path is unset");
    session
        .set_session_goal_json(&JsString::from("{\"goal\":1}"))
        .expect("in-memory goal");
    assert_eq!(
        session.get_session_goal_json().unwrap().as_ref(),
        Some(&JsString::from("{\"goal\":1}"))
    );
    session.clear_session_goal_json().expect("clear in-memory goal");
    assert_eq!(session.get_session_goal_json().unwrap(), None);

    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().display().to_string();
    let sessions = dir.path().join("sessions").display().to_string();
    let control_db = dir.path().join("control.db").display().to_string();
    let mut persisted = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&sessions)), None).unwrap();
    let cwd_before = persisted.get_cwd().to_owned();
    persisted
        .set_metadata_control_db_path(
            Some(&JsString::from(&control_db)),
            SetMetadataControlDbPathOptions {
                index_message_text: None,
                cwd_override: Some(JsString::default()),
            },
        )
        .expect("empty cwd override is ignored");
    assert_eq!(*persisted.get_cwd(), cwd_before);

    let session_file = persisted.get_session_file().unwrap().to_owned();
    write_session_metadata(
        &control_db,
        &WritableSessionMetadata {
            session_path: session_file.clone(),
            id: persisted.get_session_id().to_owned(),
            cwd: cwd_before,
            name: None,
            parent_session_path: None,
            archived_at: None,
            is_archived: None,
            is_subagent: None,
            subagent_name: None,
            model_provider: Some(String::new()),
            model_id: Some(String::new()),
            thinking_level: None,
            created_at: JsString::from("2026-01-01T00:00:00.000Z"),
            modified_at: JsString::from("2026-01-01T00:00:00.000Z"),
            message_count: 0,
            first_message: JsString::default(),
        },
    )
    .expect("seed metadata");
    let settings = persisted
        .read_persisted_session_settings()
        .expect("read settings")
        .expect("metadata row");
    assert!(settings.model.is_none(), "empty model fields count as unset");
}

#[test]
fn appending_messages_writes_the_metadata_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().display().to_string();
    let sessions = dir.path().join("sessions").display().to_string();
    let control_db = dir.path().join("control.db").display().to_string();
    let mut session = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&sessions)), None).unwrap();
    session
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let session_file = session.get_session_file().unwrap().to_owned();

    session
        .append_message(user_message("hello", 1_700_000_000_000))
        .expect("append message");
    let metadata = read_session_metadata(&control_db, &session_file)
        .expect("read metadata")
        .expect("snapshot after message append");
    assert_eq!(metadata.message_count, 1);
    assert_eq!(metadata.first_message, "hello");

    session
        .set_session_name(Some(&JsString::from("Named session")))
        .expect("set session name");
    let metadata = read_session_metadata(&control_db, &session_file)
        .expect("read metadata")
        .expect("snapshot after name write");
    assert_eq!(metadata.name.as_ref(), Some(&JsString::from("Named session")));
    assert_eq!(
        session.get_session_name().as_ref(),
        Some(&JsString::from("Named session"))
    );
}

#[test]
fn set_session_sandbox_profile_persists_and_clears() {
    use pi_coding_agent::core::permissions::presets::SandboxProfileName;

    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().display().to_string();
    let sessions = dir.path().join("sessions").display().to_string();
    let control_db = dir.path().join("control.db").display().to_string();
    let mut session = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&sessions)), None).unwrap();
    assert!(!session.set_session_sandbox_profile(None).expect("no control db"));
    session
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let session_file = session.get_session_file().unwrap().to_owned();
    session
        .append_message(user_message("hello", 1))
        .expect("append message");

    assert!(
        session
            .set_session_sandbox_profile(Some(SandboxProfileName::ReadOnly))
            .expect("write profile")
    );
    assert!(
        Path::new(&encode_node_utf8(&session_file)).exists(),
        "profile write persists for recovery"
    );
    let settings = session.read_persisted_session_settings().unwrap().unwrap();
    assert_eq!(settings.sandbox_profile, Some(SandboxProfileName::ReadOnly));

    assert!(session.set_session_sandbox_profile(None).expect("clear profile"));
    let settings = session.read_persisted_session_settings().unwrap().unwrap();
    assert_eq!(settings.sandbox_profile, None);
}

#[test]
fn metadata_backed_listing_prefers_control_db_and_excludes_subagents() {
    use pi_coding_agent::core::session_control_db::get_control_db_path;

    let dir = temp_dir("session-metadata-list");
    let session_dir = dir.join("sessions");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((&dir).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    let cwd = dir.display().to_string();
    let mut session = SessionManager::create(
        &JsString::from(&cwd),
        Some(&JsString::from(&session_dir.display().to_string())),
        None,
    )
    .unwrap();
    session
        .set_metadata_control_db_path(Some(&JsString::from(&control_db_path)), Default::default())
        .unwrap();
    session.append_message(user_message("hello", 1)).unwrap();
    session.append_message(assistant_message("hi", 2)).unwrap();
    let session_file = session.get_session_file().unwrap().to_owned();

    let subagent_path = session_dir.join("subagent.jsonl").display().to_string();
    write_session_metadata(
        &control_db_path,
        &WritableSessionMetadata {
            session_path: JsString::from(subagent_path.clone()),
            id: JsString::from("subagent"),
            cwd: JsString::from(cwd.clone()),
            name: None,
            parent_session_path: Some(session_file.clone()),
            archived_at: None,
            is_archived: None,
            is_subagent: Some(true),
            subagent_name: Some(JsString::from("worker")),
            model_provider: None,
            model_id: None,
            thinking_level: None,
            created_at: JsString::from("2026-01-01T00:00:00.000Z"),
            modified_at: JsString::from("2026-01-01T00:10:00.000Z"),
            message_count: 1,
            first_message: JsString::from("task"),
        },
    )
    .unwrap();

    let listed = SessionManager::list(
        &JsString::from(&cwd),
        Some(&JsString::from(&session_dir.display().to_string())),
        None,
        Some(&JsString::from(&control_db_path)),
    )
    .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, session_file);
    assert_eq!(listed[0].message_count, 2);
    assert_eq!(listed[0].first_message, "hello");

    let archived_path = session_dir.join("archived.jsonl").display().to_string();
    write_session_metadata(
        &control_db_path,
        &WritableSessionMetadata {
            session_path: JsString::from(archived_path),
            id: JsString::from("archived"),
            cwd: JsString::from(cwd.clone()),
            name: Some(JsString::from("Old")),
            parent_session_path: None,
            archived_at: Some("2026-01-02T00:00:00.000Z".to_owned()),
            is_archived: None,
            is_subagent: None,
            subagent_name: None,
            model_provider: None,
            model_id: None,
            thinking_level: None,
            created_at: JsString::from("2026-01-01T00:00:00.000Z"),
            modified_at: JsString::from("2026-01-01T00:10:00.000Z"),
            message_count: 3,
            first_message: JsString::from("archived"),
        },
    )
    .unwrap();

    let archived = SessionManager::list_archived(None, Some(&JsString::from(&control_db_path))).unwrap();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].id, "archived");
    assert_eq!(archived[0].name.as_ref(), Some(&JsString::from("Old")));
    assert_eq!(archived[0].is_archived, Some(true));
    assert!(
        SessionManager::list_archived(None, None).unwrap().is_empty(),
        "archived listing requires a control database"
    );

    let all = SessionManager::list_all(
        Some(&JsString::from(&session_dir.display().to_string())),
        None,
        Some(&JsString::from(&control_db_path)),
    )
    .unwrap();
    assert_eq!(all.len(), 1, "subagents and archived sessions are excluded");
    assert_eq!(all[0].path, session_file);
}

#[test]
fn metadata_listing_scopes_by_cwd_and_returns_control_db_order() {
    use pi_coding_agent::core::session_control_db::get_control_db_path;

    let dir = temp_dir("session-metadata-scope");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((&dir).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    let sessions_a = dir.join("sessions-a");
    let sessions_b = dir.join("sessions-b");
    let cwd_a = dir.join("repo-a").display().to_string();
    let cwd_b = dir.join("repo-b").display().to_string();
    for cwd in [&cwd_a, &cwd_b] {
        fs::create_dir_all(cwd).unwrap();
    }
    let mut session_a = SessionManager::create(
        &JsString::from(&cwd_a),
        Some(&JsString::from(&sessions_a.display().to_string())),
        None,
    )
    .unwrap();
    session_a
        .set_metadata_control_db_path(Some(&JsString::from(&control_db_path)), Default::default())
        .unwrap();
    session_a.append_message(user_message("a", 1)).unwrap();
    session_a.append_message(assistant_message("a", 2)).unwrap();
    let path_a = session_a.get_session_file().unwrap().to_owned();

    let mut session_b = SessionManager::create(
        &JsString::from(&cwd_b),
        Some(&JsString::from(&sessions_b.display().to_string())),
        None,
    )
    .unwrap();
    session_b
        .set_metadata_control_db_path(Some(&JsString::from(&control_db_path)), Default::default())
        .unwrap();
    session_b.append_message(user_message("b", 1)).unwrap();
    session_b.append_message(assistant_message("b", 2)).unwrap();
    let path_b = session_b.get_session_file().unwrap().to_owned();

    let scoped = SessionManager::list(
        &JsString::from(&cwd_a),
        Some(&JsString::from(&sessions_a.display().to_string())),
        None,
        Some(&JsString::from(&control_db_path)),
    )
    .unwrap();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].path, path_a);

    let all = SessionManager::list_all(None, None, Some(&JsString::from(&control_db_path))).unwrap();
    assert_eq!(all.len(), 2);
    let newest = read_session_metadata(&control_db_path, &path_b).unwrap().unwrap();
    let oldest = read_session_metadata(&control_db_path, &path_a).unwrap().unwrap();
    let expected_first = if newest.modified_at >= oldest.modified_at {
        path_b.clone()
    } else {
        path_a.clone()
    };
    assert_eq!(all[0].path, expected_first, "listing keeps control-db modified order");

    let scoped_b = SessionManager::list_all(
        Some(&JsString::from(&sessions_b.display().to_string())),
        None,
        Some(&JsString::from(&control_db_path)),
    )
    .unwrap();
    assert_eq!(scoped_b.len(), 1);
    assert_eq!(scoped_b[0].path, path_b);
}

#[test]
fn open_preserves_a_freshly_persisted_header() {
    let dir = temp_dir("session-open-header");
    let session_dir = dir.join("sessions").display().to_string();
    let mut created = SessionManager::create(
        &JsString::from(&dir.display().to_string()),
        Some(&JsString::from(&session_dir)),
        Some(NewSessionOptions {
            id: Some(JsString::from("fresh-header")),
            parent_session: Some(JsString::from("/parent/session.jsonl")),
            ..Default::default()
        }),
    )
    .unwrap();
    created.persist_for_recovery().unwrap();
    let path = created.get_session_file().unwrap().to_owned();
    let created_id = created.get_session_id().to_owned();
    drop(created);

    let opened = SessionManager::open(&path, Some(&JsString::from(&session_dir)), None).unwrap();
    assert_eq!(*opened.get_session_id(), created_id);
    assert_eq!(
        opened.get_session_name(),
        None,
        "names come from control-db metadata, not historical session_info entries"
    );
    assert_eq!(*opened.get_cwd(), dir.to_str().expect("utf-8 path"));
    let header = opened.get_header().expect("header");
    assert_eq!(*header.id(), created_id);
    assert_eq!(header.version(), Some(CURRENT_SESSION_VERSION));
    assert_eq!(header.parent_session(), Some(&JsString::from("/parent/session.jsonl")));

    let header_only_dir = temp_dir("session-open-header-only");
    let header_only_dir_path = header_only_dir.display().to_string();
    let mut header_only = SessionManager::create(
        &JsString::from(&header_only_dir_path),
        Some(&JsString::from(&header_only_dir.join("sessions").display().to_string())),
        Some(NewSessionOptions {
            id: Some(JsString::from("header-only")),
            ..Default::default()
        }),
    )
    .unwrap();
    header_only.persist_for_recovery().unwrap();
    let header_only_path = header_only.get_session_file().unwrap().to_owned();
    drop(header_only);
    let reopened = SessionManager::open(&header_only_path, None, None).unwrap();
    assert_eq!(reopened.get_session_id(), "header-only");
    assert_eq!(*reopened.get_cwd(), header_only_dir_path);
}

#[test]
fn relocation_moves_control_db_rows() {
    let dir = temp_dir("session-relocate-control-db");
    let project_a = dir.join("project-a");
    let project_b = dir.join("project-b");
    let agent_dir = dir.join("agent");
    fs::create_dir_all(&project_a).unwrap();
    fs::create_dir_all(&project_b).unwrap();
    let source_dir = get_default_session_dir(
        &JsString::from(&project_a.display().to_string()),
        Some(&JsString::from(&agent_dir.display().to_string())),
    )
    .unwrap();
    let source_file = Path::new(&encode_node_utf8(&source_dir)).join("relocating.jsonl");
    write_jsonl(
        &source_file,
        &[
            header_json("relocating-session", &project_a.display().to_string()),
            serde_json::json!({"type":"message","id":"m-1","parentId":null,"timestamp":"2026-08-04T00:00:00.000Z","message":{"role":"user","content":"hello","timestamp":1}}),
        ],
    );
    let control_db = dir.join("control.db").display().to_string();
    let mut session = SessionManager::open(
        &JsString::from(&source_file.display().to_string()),
        Some(&source_dir),
        None,
    )
    .unwrap();
    session
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let old_path = session.get_session_file().unwrap().to_owned();
    session
        .set_session_goal_json(&JsString::from("{\"goal\":\"moved\"}"))
        .expect("goal write");

    session
        .relocate(
            &JsString::from(&project_b.display().to_string()),
            Some(&JsString::from(&agent_dir.display().to_string())),
        )
        .expect("relocate");
    let new_path = session.get_session_file().unwrap().to_owned();
    assert_ne!(old_path, new_path);
    assert_eq!(
        read_session_goal(&control_db, &new_path).expect("read moved goal"),
        (Some("{\"goal\":\"moved\"}".to_owned())).map(JsString::from)
    );
    assert_eq!(
        read_session_goal(&control_db, &old_path).expect("read old goal"),
        None,
        "control-db rows move with the session file"
    );
}

#[test]
fn opening_an_empty_file_resets_names_without_restoring_stale_metadata() {
    let dir = tempfile::tempdir().expect("fixture");
    let cwd = dir.path().display().to_string();
    let db = dir.path().join("control.sqlite").display().to_string();
    let empty = dir.path().join("empty.jsonl").display().to_string();
    fs::write(&empty, "").expect("empty session file");
    write_session_metadata(&db, &metadata_row(&empty, "stale", &cwd, Some("Stale Name"))).expect("stale metadata");
    let mut manager =
        SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&cwd)), None).expect("manager");
    manager
        .set_metadata_control_db_path(Some(&JsString::from(&db)), Default::default())
        .expect("attach metadata");
    manager
        .set_session_name(Some(&JsString::from("Original Name")))
        .expect("original name");
    manager
        .set_session_file(&JsString::from(&empty))
        .expect("initialize empty file");
    assert_eq!(manager.get_session_name(), None);
    assert!(!manager.has_session_name_state());
    assert_ne!(manager.get_session_id(), "stale");
}
