use std::fs;
use std::path::{Path, PathBuf};

use pi_coding_agent::core::external_session_importer::{
    ExternalSessionImportOptions, ExternalSessionProvider, import_external_session_alias, is_external_session_alias,
};
use pi_coding_agent::core::session_manager::SessionManager;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pi-rust-{name}-{}", pi_ai::utils::uuid::random_uuid()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_jsonl(path: &Path, entries: &[serde_json::Value]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let content = entries
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, format!("{content}\n")).unwrap();
}

fn read_jsonl(path: &str) -> Vec<serde_json::Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn recognizes_external_aliases() {
    assert!(is_external_session_alias("codex/abc"));
    assert!(is_external_session_alias("claude/abc"));
    assert!(!is_external_session_alias("codex/"));
    assert!(!is_external_session_alias("other/abc"));
    assert!(!is_external_session_alias("codex/a/b"));
    assert!(!is_external_session_alias("abc"));
}

#[test]
fn imports_a_codex_session_alias() {
    let home = temp_dir("import-codex");
    let project = home.join("project");
    let session_dir = home.join("sessions");
    fs::create_dir_all(&project).unwrap();
    let session_id = "019de1a5-95a6-7793-a098-68abf8e21e9e";
    let codex_dir = home.join(".codex").join("sessions").join("2026").join("04").join("30");
    write_jsonl(
        &codex_dir.join(format!("rollout-2026-04-30T22-46-58-{session_id}.jsonl")),
        &[
            serde_json::json!({
                "timestamp": "2026-05-01T03:46:59.100Z",
                "type": "session_meta",
                "payload": {"id": session_id, "cwd": project.display().to_string(), "model_provider": "openai"},
            }),
            serde_json::json!({
                "timestamp": "2026-05-01T03:47:00.000Z",
                "type": "response_item",
                "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hello"}]},
            }),
            serde_json::json!({
                "timestamp": "2026-05-01T03:47:01.000Z",
                "type": "response_item",
                "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "hi"}]},
            }),
        ],
    );

    let imported = import_external_session_alias(
        &format!("codex/{session_id}"),
        &ExternalSessionImportOptions {
            home_dir: Some(home.display().to_string()),
            session_dir: Some(session_dir.display().to_string()),
            fallback_cwd: None,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(imported.provider, ExternalSessionProvider::Codex);
    assert_eq!(
        imported.path,
        session_dir
            .join(format!("codex-{session_id}.jsonl"))
            .display()
            .to_string()
    );
    let manager = SessionManager::open(&imported.path, None, None).unwrap();
    assert_eq!(manager.get_cwd(), project.display().to_string());
    assert_eq!(manager.get_session_id(), format!("codex-{session_id}"));
    let context = manager.build_session_context();
    let roles: Vec<&str> = context
        .messages
        .iter()
        .map(pi_agent_core::agent::AgentMessage::role)
        .collect();
    assert_eq!(roles, ["user", "assistant"]);
}

#[test]
fn imports_a_claude_session_alias() {
    let home = temp_dir("import-claude");
    let project = home.join("project");
    let session_dir = home.join("sessions");
    fs::create_dir_all(&project).unwrap();
    let session_id = "11111111-2222-4333-8444-555555555555";
    let claude_dir = home.join(".claude").join("projects").join("-tmp-project");
    write_jsonl(
        &claude_dir.join(format!("{session_id}.jsonl")),
        &[
            serde_json::json!({"type": "summary", "summary": "Old summary", "cwd": project.display().to_string(), "sessionId": session_id}),
            serde_json::json!({
                "type": "user",
                "timestamp": "2026-05-01T03:47:00.000Z",
                "cwd": project.display().to_string(),
                "message": {"role": "user", "content": [{"type": "text", "text": "hello claude"}]},
            }),
            serde_json::json!({
                "type": "assistant",
                "timestamp": "2026-05-01T03:47:01.000Z",
                "message": {"role": "assistant", "content": [{"type": "text", "text": "hi claude"}]},
            }),
        ],
    );

    let imported = import_external_session_alias(
        &format!("claude/{session_id}"),
        &ExternalSessionImportOptions {
            home_dir: Some(home.display().to_string()),
            session_dir: Some(session_dir.display().to_string()),
            fallback_cwd: None,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(imported.provider, ExternalSessionProvider::Claude);
    let entries = read_jsonl(&imported.path);
    assert_eq!(entries[0]["type"], "session");
    assert_eq!(entries[0]["id"], format!("claude-{session_id}"));
    assert_eq!(entries[0]["cwd"], project.display().to_string());
    assert_eq!(entries[1]["message"]["role"], "user");
    assert_eq!(entries[2]["message"]["role"], "assistant");
}

#[test]
fn reuses_an_existing_imported_session() {
    let home = temp_dir("import-reuse");
    let project = home.join("project");
    let session_dir = home.join("sessions");
    fs::create_dir_all(&project).unwrap();
    let session_id = "reuse-session";
    write_jsonl(
        &home
            .join(".codex")
            .join("sessions")
            .join(format!("rollout-2026-04-30T22-46-58-{session_id}.jsonl")),
        &[
            serde_json::json!({"type": "session_meta", "payload": {"id": session_id, "cwd": project.display().to_string()}}),
        ],
    );
    let options = ExternalSessionImportOptions {
        home_dir: Some(home.display().to_string()),
        session_dir: Some(session_dir.display().to_string()),
        fallback_cwd: None,
    };
    let first = import_external_session_alias(&format!("codex/{session_id}"), &options)
        .unwrap()
        .unwrap();
    let second = import_external_session_alias(&format!("codex/{session_id}"), &options)
        .unwrap()
        .unwrap();
    assert_eq!(first.path, second.path);
}

#[test]
fn falls_back_to_the_provided_cwd_and_normalizes_string_content() {
    let home = temp_dir("import-fallback");
    let fallback = home.join("fallback");
    fs::create_dir_all(&fallback).unwrap();
    let session_dir = home.join("sessions");
    let session_id = "no-cwd";
    write_jsonl(
        &home
            .join(".claude")
            .join("projects")
            .join("-tmp")
            .join(format!("{session_id}.jsonl")),
        &[
            serde_json::json!({"type": "user", "message": {"role": "user", "content": "plain string"}}),
            serde_json::json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "image", "data": "x"}]}}),
        ],
    );
    let imported = import_external_session_alias(
        &format!("claude/{session_id}"),
        &ExternalSessionImportOptions {
            home_dir: Some(home.display().to_string()),
            session_dir: Some(session_dir.display().to_string()),
            fallback_cwd: Some(fallback.display().to_string()),
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(imported.cwd, fallback.display().to_string());
    let manager = SessionManager::open(&imported.path, None, None).unwrap();
    assert_eq!(manager.build_session_context().messages.len(), 1);
    assert_eq!(manager.build_session_context().messages[0].role(), "user");
}

#[test]
fn returns_none_for_missing_aliases_and_sources() {
    let home = temp_dir("import-missing");
    let options = ExternalSessionImportOptions {
        home_dir: Some(home.display().to_string()),
        session_dir: Some(home.join("sessions").display().to_string()),
        fallback_cwd: None,
    };
    assert!(import_external_session_alias("codex/none", &options).unwrap().is_none());
    assert!(
        import_external_session_alias("not-an-alias", &options)
            .unwrap()
            .is_none()
    );
}
