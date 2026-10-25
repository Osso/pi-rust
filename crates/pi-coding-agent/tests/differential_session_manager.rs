#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::fs;
use std::path::PathBuf;

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, Usage, UserContent, UserMessage,
    UserRole,
};
use pi_coding_agent::core::session_manager::{
    FileEntry, LeafSelection, SessionEntry, SessionManager, build_context_entries, build_session_context,
    find_most_recent_session, get_default_session_dir_path, load_entries_from_file, parse_session_entries,
    read_session_header,
};
use pi_coding_agent::core::session_tool_output::{
    serialize_session_entry_for_persistence, truncate_tool_result_for_persistence,
};
use serde_json::{Value, json};

const REQUEST_BASE: u64 = 41_000;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pi-rust-diff-{name}-{}",
        pi_ai::utils::random_uuid::random_uuid().unwrap()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

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

fn entries_from_values(values: &[Value]) -> Vec<SessionEntry> {
    values
        .iter()
        .filter_map(|value| match serde_json::from_value::<FileEntry>(value.clone()).ok()? {
            FileEntry::Entry(entry) => Some(entry),
            FileEntry::Header(_) | FileEntry::Json(_) => None,
        })
        .collect()
}

fn context_value(entries: &[SessionEntry], leaf: LeafSelection<'_>) -> Value {
    let context = build_session_context(entries, leaf);
    json!({
        "messages": context.messages,
        "thinkingLevel": context.thinking_level,
        "model": context.model.map(|model| json!({"provider": model.provider, "modelId": model.model_id})),
    })
}

fn serialize_entries(entries: &[Value]) -> Vec<String> {
    entries
        .iter()
        .map(|value| {
            let entry: FileEntry = serde_json::from_value(value.clone()).unwrap();
            serialize_session_entry_for_persistence(&entry).unwrap()
        })
        .collect()
}

fn file_entry_lines(entries: &[FileEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect()
}

fn session_entry_lines(entries: &[SessionEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect()
}

fn message_lines(messages: &[AgentMessage]) -> Vec<String> {
    messages
        .iter()
        .map(|message| serde_json::to_string(message).unwrap())
        .collect()
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap().to_owned())
        .collect()
}

fn normalize_entry_ids(entries: &mut [Value]) -> Vec<String> {
    let mut ids = std::collections::HashMap::new();
    for entry in entries.iter() {
        if let Some(id) = entry.get("id").and_then(Value::as_str) {
            let placeholder = format!("id{}", ids.len());
            ids.insert(id.to_owned(), placeholder);
        }
    }
    for entry in entries.iter_mut() {
        let Some(object) = entry.as_object_mut() else {
            continue;
        };
        if let Some(id) = object
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| ids.get(id))
            .cloned()
        {
            object.insert("id".to_owned(), Value::String(id));
        }
        for key in ["parentId", "firstKeptEntryId"] {
            if let Some(mapped) = object
                .get(key)
                .and_then(Value::as_str)
                .and_then(|value| ids.get(value))
                .cloned()
            {
                object.insert(key.to_owned(), Value::String(mapped));
            }
        }
    }
    entries
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect()
}

#[test]
fn serializes_entries_byte_identically() {
    let mut oracle = oracle::TsOracle::spawn(
        "session-manager-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let fixtures = vec![
        json!({"type":"session","version":3,"id":"s1","timestamp":"2025-01-01T00:00:00.000Z","cwd":"/tmp"}),
        json!({"type":"session","id":"s1","timestamp":"2025-01-01T00:00:00.000Z","cwd":"/tmp","parentSession":"/p.jsonl"}),
        json!({"type":"message","id":"m1","parentId":null,"timestamp":"2025-01-01T00:00:00.000Z","message":{"role":"user","content":"hello","timestamp":1}}),
        json!({"type":"message","id":"m2","parentId":"m1","timestamp":"2025-01-01T00:00:00.000Z","message":{"role":"assistant","content":[{"type":"text","text":"hi"}],"api":"anthropic-messages","provider":"anthropic","model":"claude-test","usage":{"input":1,"output":2,"cacheRead":3,"cacheWrite":4,"totalTokens":10,"cost":{"input":0.1,"output":0.2,"cacheRead":0.3,"cacheWrite":0.4,"total":1}},"stopReason":"stop","timestamp":2}}),
        json!({"type":"message","id":"m3","parentId":"m2","timestamp":"2025-01-01T00:00:00.000Z","message":{"role":"toolResult","toolCallId":"call","toolName":"tool","content":[{"type":"text","text":"ok"}],"isError":false,"timestamp":3}}),
        json!({"type":"thinking_level_change","id":"t1","parentId":"m3","timestamp":"2025-01-01T00:00:00.000Z","thinkingLevel":"high"}),
        json!({"type":"model_change","id":"mc1","parentId":"t1","timestamp":"2025-01-01T00:00:00.000Z","provider":"openai","modelId":"gpt-4"}),
        json!({"type":"compaction","id":"c1","parentId":"mc1","timestamp":"2025-01-01T00:00:00.000Z","summary":"summary","firstKeptEntryId":"m1","tokensBefore":1000}),
        json!({"type":"compaction","id":"c2","parentId":"c1","timestamp":"2025-01-01T00:00:00.000Z","summary":"summary","firstKeptEntryId":"m1","tokensBefore":1000.5,"usage":{"input":1,"output":2,"cacheRead":3,"cacheWrite":4,"totalTokens":10,"cost":{"input":0.1,"output":0.2,"cacheRead":0.3,"cacheWrite":0.4,"total":1}},"durationMs":321,"details":{"a":1},"fromHook":true}),
        json!({"type":"branch_summary","id":"b1","parentId":"c2","timestamp":"2025-01-01T00:00:00.000Z","fromId":"m1","summary":"branch","details":{"x":true},"fromHook":false}),
        json!({"type":"custom","customType":"state","data":{"k":"v"},"id":"cu1","parentId":"b1","timestamp":"2025-01-01T00:00:00.000Z"}),
        json!({"type":"custom_message","customType":"cwd_changed","content":"moved","display":true,"details":{"cwd":"/tmp"},"id":"cm1","parentId":"cu1","timestamp":"2025-01-01T00:00:00.000Z"}),
        json!({"type":"label","id":"l1","parentId":"cm1","timestamp":"2025-01-01T00:00:00.000Z","targetId":"m1","label":"checkpoint"}),
        json!({"type":"label","id":"l2","parentId":"l1","timestamp":"2025-01-01T00:00:00.000Z","targetId":"m1"}),
        json!({"type":"session_info","id":"si1","parentId":"l2","timestamp":"2025-01-01T00:00:00.000Z","name":"title"}),
        json!({"type":"session_info","id":"si2","parentId":"si1","timestamp":"2025-01-01T00:00:00.000Z"}),
        json!({"type":"future_entry","id":"f1","parentId":"si2","timestamp":"2025-01-01T00:00:00.000Z","payload":{"n":1}}),
        json!({"type":"custom_message","customType":"emoji","content":"😀漢字","display":false,"id":"cm2","parentId":"f1","timestamp":"2025-01-01T00:00:00.000Z"}),
    ];
    let response = oracle.request(&json!({
        "id": REQUEST_BASE,
        "operation": "serialize_entries",
        "entries": fixtures,
    }));
    assert_eq!(response["ok"], true, "{response}");
    let expected: Vec<String> = response["output"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(serialize_entries(&fixtures), expected);
}

#[test]
fn parses_and_builds_context_identically() {
    let mut oracle = oracle::TsOracle::spawn(
        "session-manager-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let content = "{\"type\":\"session\",\"version\":3,\"id\":\"s1\",\"timestamp\":\"2025-01-01T00:00:00.000Z\",\"cwd\":\"/tmp\"}\n\
        not json\n\
        {\"type\":\"message\",\"id\":\"1\",\"parentId\":null,\"timestamp\":\"2025-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"first\",\"timestamp\":1}}\n\
        {\"type\":\"message\",\"id\":\"2\",\"parentId\":\"1\",\"timestamp\":\"2025-01-01T00:00:00.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"response1\"}],\"api\":\"anthropic-messages\",\"provider\":\"anthropic\",\"model\":\"claude-test\",\"usage\":{\"input\":1,\"output\":1,\"cacheRead\":0,\"cacheWrite\":0,\"totalTokens\":2,\"cost\":{\"input\":0,\"output\":0,\"cacheRead\":0,\"cacheWrite\":0,\"total\":0}},\"stopReason\":\"stop\",\"timestamp\":1}}\n\
        {\"type\":\"message\",\"id\":\"3\",\"parentId\":\"2\",\"timestamp\":\"2025-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"second\",\"timestamp\":1}}\n\
        {\"type\":\"compaction\",\"id\":\"4\",\"parentId\":\"3\",\"timestamp\":\"2025-01-01T00:00:00.000Z\",\"summary\":\"Summary\",\"firstKeptEntryId\":\"2\",\"tokensBefore\":1000}\n\
        {\"type\":\"message\",\"id\":\"5\",\"parentId\":\"4\",\"timestamp\":\"2025-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"third\",\"timestamp\":1}}\n\
        {\"type\":\"custom\",\"customType\":\"state\",\"data\":{\"k\":\"v\"},\"id\":\"6\",\"parentId\":\"5\",\"timestamp\":\"2025-01-01T00:00:00.000Z\"}\n";
    let response = oracle.request(&json!({"id": REQUEST_BASE + 1, "operation": "parse_entries", "content": content}));
    assert_eq!(response["ok"], true, "{response}");
    let parsed_entries = parse_session_entries(content);
    let rust_parsed: Vec<Value> = parsed_entries
        .iter()
        .map(|entry| serde_json::to_value(entry).unwrap())
        .collect();
    assert_eq!(Value::Array(rust_parsed), response["output"]["entries"]);
    assert_eq!(
        file_entry_lines(&parsed_entries),
        string_array(&response["output"]["entryLines"])
    );

    let entries_value = response["output"]["entries"].clone();
    let entries = entries_from_values(entries_value.as_array().unwrap());
    for leaf in [None, Some("3"), Some("4"), Some("missing"), Some("")] {
        let request = match leaf {
            None => json!({"id": REQUEST_BASE + 2, "operation": "build_context", "entries": entries_value}),
            Some(leaf) => {
                json!({"id": REQUEST_BASE + 2, "operation": "build_context", "entries": entries_value, "leafId": leaf})
            }
        };
        let response = oracle.request(&request);
        assert_eq!(response["ok"], true, "{response}");
        let rust_leaf = match leaf {
            None | Some("") => LeafSelection::Last,
            Some(id) => LeafSelection::Id(&JsString::from(id)),
        };
        let rust_ids: Vec<JsString> = build_context_entries(&entries, rust_leaf)
            .iter()
            .map(|entry| entry.id().cloned().unwrap_or_default())
            .collect();
        assert_eq!(
            JsValue::from(json!(response["output"]["contextEntries"])),
            pi_js_compat::js_json::to_value(&rust_ids).unwrap()
        );
        assert_eq!(
            context_value(&entries, rust_leaf),
            response["output"]["context"],
            "leaf {leaf:?}"
        );
        let context = build_session_context(&entries, rust_leaf);
        assert_eq!(
            message_lines(&context.messages),
            string_array(&response["output"]["contextMessageLines"]),
            "leaf {leaf:?}"
        );
    }

    let response = oracle.request(
        &json!({"id": REQUEST_BASE + 3, "operation": "build_context", "entries": entries_value, "leafId": Value::Null}),
    );
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(json!(Vec::<String>::new()), response["output"]["contextEntries"]);
    assert_eq!(
        context_value(&entries, LeafSelection::None),
        response["output"]["context"]
    );
}

#[test]
fn reads_files_and_headers_identically() {
    let mut oracle = oracle::TsOracle::spawn(
        "session-manager-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let dir = temp_dir("files");
    let file = dir.join("session.jsonl");
    fs::write(
        &file,
        "{\"type\":\"session\",\"version\":3,\"id\":\"abc\",\"timestamp\":\"2025-01-01T00:00:00.000Z\",\"cwd\":\"/tmp\"}\n\
         {\"type\":\"message\",\"id\":\"1\",\"parentId\":null,\"timestamp\":\"2025-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"hi\",\"timestamp\":1}}\n",
    )
    .unwrap();

    let response = oracle
        .request(&json!({"id": REQUEST_BASE + 10, "operation": "load_entries", "path": file.display().to_string()}));
    let loaded = load_entries_from_file(&JsString::from(&file.display().to_string())).unwrap();
    let rust_entries: Vec<Value> = loaded
        .iter()
        .map(|entry| serde_json::to_value(entry).unwrap())
        .collect();
    assert_eq!(Value::Array(rust_entries), response["output"]["entries"]);
    assert_eq!(
        file_entry_lines(&loaded),
        string_array(&response["output"]["entryLines"])
    );

    let response = oracle
        .request(&json!({"id": REQUEST_BASE + 11, "operation": "read_header", "path": file.display().to_string()}));
    let header = read_session_header(&JsString::from(&file.display().to_string())).unwrap();
    assert_eq!(serde_json::to_value(&header).unwrap(), response["output"]["header"]);
    assert_eq!(
        serde_json::to_string(&header).unwrap(),
        response["output"]["headerJson"].as_str().unwrap()
    );

    let response = oracle
        .request(&json!({"id": REQUEST_BASE + 12, "operation": "find_most_recent", "dir": dir.display().to_string()}));
    assert_eq!(
        json!(find_most_recent_session(&JsString::from(&dir.display().to_string()), None).unwrap()),
        response["output"]["path"]
    );

    let agent_dir = dir.join("agent");
    let cwd = dir.join("project");
    fs::create_dir_all(&cwd).unwrap();
    let response = oracle.request(&json!({"id": REQUEST_BASE + 13, "operation": "default_session_dir", "cwd": cwd.display().to_string(), "agentDir": agent_dir.display().to_string()}));
    assert_eq!(
        json!(
            pi_coding_agent::core::session_manager::get_default_session_dir(
                &JsString::from(&cwd.display().to_string()),
                Some(&JsString::from(&agent_dir.display().to_string()))
            )
            .unwrap()
        ),
        response["output"]["path"]
    );
    assert_eq!(
        json!(
            get_default_session_dir_path(
                &JsString::from(&cwd.display().to_string()),
                &JsString::from(&agent_dir.display().to_string())
            )
            .unwrap()
        ),
        response["output"]["path"]
    );
}

#[test]
fn reads_typescript_written_sessions_and_appends_visible_to_typescript() {
    let mut oracle = oracle::TsOracle::spawn(
        "session-manager-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let dir = temp_dir("cross-write");
    let response = oracle.request(&json!({
        "id": REQUEST_BASE + 20,
        "operation": "write_session",
        "cwd": dir.display().to_string(),
        "dir": dir.display().to_string(),
        "id": "ts-session",
        "messages": [user_message("hello from ts", 1), assistant_message("hi from ts", 2)],
    }));
    assert_eq!(response["ok"], true, "{response}");
    let path = response["output"]["path"].as_str().unwrap().to_owned();

    let rust = SessionManager::open(
        &JsString::from(&path),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    let rust_entries: Vec<Value> = rust
        .get_entries()
        .iter()
        .map(|entry| serde_json::to_value(entry).unwrap())
        .collect();
    let oracle_read = oracle.request(&json!({"id": REQUEST_BASE + 21, "operation": "read_session", "path": path, "sessionDir": dir.display().to_string()}));
    assert_eq!(Value::Array(rust_entries), oracle_read["output"]["entries"]);
    assert_eq!(
        session_entry_lines(&rust.get_entries()),
        string_array(&oracle_read["output"]["entryLines"])
    );
    assert_eq!(
        message_lines(&rust.build_session_context().messages),
        string_array(&oracle_read["output"]["contextMessageLines"])
    );
    assert_eq!(json!(rust.get_session_id()), oracle_read["output"]["id"]);
    assert_eq!(json!(rust.get_cwd()), oracle_read["output"]["cwd"]);
    assert_eq!(json!(rust.get_leaf_id()), oracle_read["output"]["leafId"]);
    assert_eq!(
        json!(rust.build_session_context().messages),
        oracle_read["output"]["context"]["messages"]
    );

    let mut rust = rust;
    rust.append_message(user_message("appended by rust", 3)).unwrap();
    rust.append_message(assistant_message("reply by rust", 4)).unwrap();
    let oracle_after = oracle.request(&json!({"id": REQUEST_BASE + 22, "operation": "read_session", "path": path, "sessionDir": dir.display().to_string()}));
    let rust_entries: Vec<Value> = rust
        .get_entries()
        .iter()
        .map(|entry| serde_json::to_value(entry).unwrap())
        .collect();
    assert_eq!(Value::Array(rust_entries), oracle_after["output"]["entries"]);
    assert_eq!(
        session_entry_lines(&rust.get_entries()),
        string_array(&oracle_after["output"]["entryLines"])
    );
    assert_eq!(
        message_lines(&rust.build_session_context().messages),
        string_array(&oracle_after["output"]["contextMessageLines"])
    );
    assert_eq!(oracle_after["output"]["entries"].as_array().unwrap().len(), 4);
}

#[test]
fn typescript_reads_rust_written_sessions() {
    let mut oracle = oracle::TsOracle::spawn(
        "session-manager-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let dir = temp_dir("cross-rust-write");
    let mut manager = SessionManager::create(
        &JsString::from(&dir.display().to_string()),
        Some(&JsString::from(&dir.display().to_string())),
        Some(pi_coding_agent::core::session_manager::NewSessionOptions {
            id: Some(JsString::from("rust-session")),
            ..pi_coding_agent::core::session_manager::NewSessionOptions::default()
        }),
    )
    .unwrap();
    manager.append_message(user_message("hello from rust", 1)).unwrap();
    manager.append_message(assistant_message("hi from rust", 2)).unwrap();
    manager
        .append_custom_entry(&JsString::from("state"), Some(JsValue::from(json!({"k":"v"}))))
        .unwrap();
    let leaf_id = manager.get_leaf_id().unwrap().to_owned();
    manager
        .append_label_change(&leaf_id, Some(&JsString::from("checkpoint")))
        .unwrap();
    let path = manager.get_session_file().unwrap().to_owned();

    let response = oracle.request(&json!({"id": REQUEST_BASE + 30, "operation": "read_session", "path": path, "sessionDir": dir.display().to_string()}));
    assert_eq!(response["ok"], true, "{response}");
    let rust_entries: Vec<Value> = manager
        .get_entries()
        .iter()
        .map(|entry| serde_json::to_value(entry).unwrap())
        .collect();
    assert_eq!(Value::Array(rust_entries), response["output"]["entries"]);
    assert_eq!(
        session_entry_lines(&manager.get_entries()),
        string_array(&response["output"]["entryLines"])
    );
    assert_eq!(
        message_lines(&manager.build_session_context().messages),
        string_array(&response["output"]["contextMessageLines"])
    );
    assert_eq!(json!(manager.get_session_id()), response["output"]["id"]);
    assert_eq!(json!(manager.get_cwd()), response["output"]["cwd"]);
    assert_eq!(
        json!(manager.build_session_context().messages),
        response["output"]["context"]["messages"]
    );
}

#[test]
fn truncates_tool_results_identically() {
    let mut oracle = oracle::TsOracle::spawn(
        "session-manager-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let fixtures = vec![
        json!({"type":"message","id":"t1","parentId":null,"timestamp":"2025-01-01T00:00:00.000Z","message":{"role":"toolResult","toolCallId":"call","toolName":"bash","content":[{"type":"text","text":"small"}],"isError":false,"timestamp":1}}),
        json!({"type":"message","id":"t2","parentId":null,"timestamp":"2025-01-01T00:00:00.000Z","message":{"role":"toolResult","toolCallId":"call","toolName":"bash","content":[{"type":"text","text":"😀漢字"}],"isError":false,"timestamp":1}}),
    ];
    for fixture in fixtures {
        let response =
            oracle.request(&json!({"id": REQUEST_BASE + 40, "operation": "truncate_tool_result", "entry": fixture}));
        let entry: SessionEntry = match serde_json::from_value::<FileEntry>(fixture.clone()).unwrap() {
            FileEntry::Entry(entry) => entry,
            FileEntry::Header(_) | FileEntry::Json(_) => unreachable!(),
        };
        let (truncated, _) = truncate_tool_result_for_persistence(&entry).unwrap();
        assert_eq!(serde_json::to_value(&truncated).unwrap(), response["output"]["entry"]);
        assert_eq!(
            serde_json::to_string(&truncated).unwrap(),
            response["output"]["entryJson"].as_str().unwrap()
        );
    }
}

#[test]
fn migration_matches_typescript_structure() {
    let mut oracle = oracle::TsOracle::spawn(
        "session-manager-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let entries = vec![
        json!({"type":"session","id":"sess-1","timestamp":"2025-01-01T00:00:00.000Z","cwd":"/tmp"}),
        json!({"type":"message","timestamp":"2025-01-01T00:00:01.000Z","message":{"role":"user","content":"hi","timestamp":1}}),
        json!({"type":"message","timestamp":"2025-01-01T00:00:02.000Z","message":{"role":"hookMessage","customType":"x","content":"y","display":true,"timestamp":2}}),
        json!({"type":"compaction","timestamp":"2025-01-01T00:00:03.000Z","summary":"s","firstKeptEntryIndex":1,"tokensBefore":1}),
    ];
    let response =
        oracle.request(&json!({"id": REQUEST_BASE + 50, "operation": "migrate_entries", "entries": entries.clone()}));
    assert_eq!(response["ok"], true, "{response}");
    let mut rust_entries: Vec<FileEntry> = entries
        .iter()
        .map(|value| serde_json::from_value(value.clone()).unwrap())
        .collect();
    pi_coding_agent::core::session_manager::migrate_session_entries(&mut rust_entries).unwrap();
    let rust: Value = serde_json::to_value(&rust_entries).unwrap();
    let ts = &response["output"]["entries"];

    let mut rust_values = rust.as_array().unwrap().clone();
    let mut ts_values = ts.as_array().unwrap().clone();
    // Ids are generated randomly in both implementations; substitute them in place.
    let rust_lines = normalize_entry_ids(&mut rust_values);
    let ts_lines = normalize_entry_ids(&mut ts_values);
    assert_eq!(rust_lines, ts_lines);
    assert_eq!(rust_values, ts_values);
}

#[test]
fn cross_implementation_appends_round_trip() {
    let mut oracle = oracle::TsOracle::spawn(
        "session-manager-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let dir = temp_dir("cross-append");
    let response = oracle.request(&json!({
        "id": REQUEST_BASE + 60,
        "operation": "write_session",
        "cwd": dir.display().to_string(),
        "dir": dir.display().to_string(),
        "id": "append-session",
        "messages": [user_message("first", 1), assistant_message("second", 2)],
    }));
    let path = response["output"]["path"].as_str().unwrap().to_owned();
    let mut manager = SessionManager::open(
        &JsString::from(&path),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    manager.append_message(user_message("third", 3)).unwrap();
    manager.append_message(assistant_message("fourth", 4)).unwrap();
    let response = oracle.request(&json!({"id": REQUEST_BASE + 61, "operation": "append_message", "path": path, "sessionDir": dir.display().to_string(), "message": user_message("fifth", 5)}));
    assert_eq!(response["ok"], true, "{response}");
    manager.append_message(assistant_message("sixth", 6)).unwrap();
    let reopened = SessionManager::open(
        &JsString::from(&path),
        Some(&JsString::from(&dir.display().to_string())),
        None,
    )
    .unwrap();
    assert_eq!(reopened.get_entries().len(), 6);
    let context = reopened.build_session_context();
    let roles: Vec<JsString> = context.messages.iter().map(AgentMessage::role).collect();
    // The TypeScript-appended user message became a sibling of the leaf chain, so the
    // active slice excludes it; both implementations report the same five entries.
    assert_eq!(roles, ["user", "assistant", "user", "assistant", "assistant"]);
    let oracle_read = oracle.request(&json!({"id": REQUEST_BASE + 62, "operation": "read_session", "path": path, "sessionDir": dir.display().to_string()}));
    let rust_entries: Vec<Value> = reopened
        .get_entries()
        .iter()
        .map(|entry| serde_json::to_value(entry).unwrap())
        .collect();
    assert_eq!(Value::Array(rust_entries), oracle_read["output"]["entries"]);
}

#[test]
fn in_memory_goal_accessors_match_typescript() {
    let ts_dir = temp_dir("diff-goal-ts");
    let rust_dir = temp_dir("diff-goal-rust");
    let ts_cwd = ts_dir.join("cwd").display().to_string();
    let rust_cwd = rust_dir.join("cwd").display().to_string();
    fs::create_dir_all(&ts_cwd).unwrap();
    fs::create_dir_all(&rust_cwd).unwrap();

    let mut oracle = oracle::TsOracle::spawn(
        "session-manager-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let response = oracle.request(&json!({
        "id": REQUEST_BASE + 70,
        "operation": "goal_in_memory",
        "cwd": ts_cwd,
        "sessionDir": ts_dir.join("sessions").display().to_string(),
    }));
    assert_eq!(response["ok"], true, "{response}");
    let expected = response["output"]["values"].clone();

    let mut manager = SessionManager::create(
        &JsString::from(&rust_cwd),
        Some(&JsString::from(&rust_dir.join("sessions").display().to_string())),
        None,
    )
    .expect("session manager");
    manager
        .set_session_goal_json(&JsString::from("{\"goal\":1}"))
        .expect("set in-memory goal");
    let set = json!(manager.get_session_goal_json().expect("read goal"));
    let for_session = json!(
        manager
            .get_session_goal_json_for_session(&JsString::from("/tmp/other-session.jsonl"))
            .expect("read other goal")
    );
    let persisted = match manager.read_persisted_session_settings().expect("read settings") {
        None => Value::Null,
        Some(settings) => json!({
            "model": settings.model.map(|model| json!({"provider": model.provider, "modelId": model.model_id})),
            "sandboxProfile": settings.sandbox_profile,
            "thinkingLevel": settings.thinking_level,
        }),
    };
    manager.clear_session_goal_json().expect("clear goal");
    let cleared = json!(manager.get_session_goal_json().expect("read cleared goal"));
    manager
        .set_metadata_control_db_path(
            None,
            pi_coding_agent::core::session_manager::SetMetadataControlDbPathOptions {
                index_message_text: None,
                cwd_override: Some(JsString::default()),
            },
        )
        .expect("empty cwd override");
    let actual = json!([set, for_session, persisted, cleared, manager.get_cwd().to_owned()]);
    assert_eq!(actual.as_array().unwrap()[..4], expected.as_array().unwrap()[..4]);
    assert_eq!(expected[4], json!(ts_cwd), "TypeScript keeps its cwd");
    assert_eq!(actual[4], json!(rust_cwd), "empty cwdOverride is ignored");
}
