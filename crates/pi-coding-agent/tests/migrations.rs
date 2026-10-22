use pi_js_compat::js_json::JsString;
use std::fs;

use pi_coding_agent::migrations::{
    encode_session_directory_name, migrate_auth_to_auth_json_in, migrate_sessions_from_agent_root_in,
    migrate_tools_to_bin_in, run_migrations_in,
};
use serde_json::json;
use tempfile::TempDir;

fn read(path: &std::path::Path) -> String {
    fs::read_to_string(path).expect("fixture file must be readable")
}

#[test]
fn migrates_oauth_json_and_renames_it() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("oauth.json"),
        r#"{"anthropic":{"refresh":"r1","access":"a1"},"openai":"not-a-record"}"#,
    )
    .unwrap();

    let providers = migrate_auth_to_auth_json_in(temp.path());

    assert_eq!(providers, vec!["anthropic".to_owned()]);
    assert_eq!(
        read(&temp.path().join("auth.json")),
        "{\n  \"anthropic\": {\n    \"type\": \"oauth\",\n    \"refresh\": \"r1\",\n    \"access\": \"a1\"\n  }\n}"
    );
    assert!(!temp.path().join("oauth.json").exists());
    assert_eq!(
        read(&temp.path().join("oauth.json.migrated")),
        r#"{"anthropic":{"refresh":"r1","access":"a1"},"openai":"not-a-record"}"#
    );
}

#[test]
fn migrates_settings_api_keys_and_removes_the_field() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("settings.json"),
        r#"{"theme":"dark","apiKeys":{"openai":"sk-1","bad":42},"packages":["npm:x"]}"#,
    )
    .unwrap();

    let providers = migrate_auth_to_auth_json_in(temp.path());

    assert_eq!(providers, vec!["openai".to_owned()]);
    assert_eq!(
        read(&temp.path().join("settings.json")),
        "{\n  \"theme\": \"dark\",\n  \"packages\": [\n    \"npm:x\"\n  ]\n}"
    );
    assert_eq!(
        read(&temp.path().join("auth.json")),
        "{\n  \"openai\": {\n    \"type\": \"api_key\",\n    \"key\": \"sk-1\"\n  }\n}"
    );
}

#[test]
fn keeps_existing_auth_entries_and_skips_them_in_the_provider_list() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("auth.json"),
        r#"{"anthropic":{"type":"api_key","key":"existing"}}"#,
    )
    .unwrap();
    fs::write(
        temp.path().join("oauth.json"),
        r#"{"anthropic":{"refresh":"new"},"openai":{"refresh":"other"}}"#,
    )
    .unwrap();

    let providers = migrate_auth_to_auth_json_in(temp.path());

    assert_eq!(providers, vec!["openai".to_owned()]);
    assert_eq!(
        read(&temp.path().join("auth.json")),
        "{\n  \"anthropic\": {\n    \"type\": \"api_key\",\n    \"key\": \"existing\"\n  },\n  \"openai\": {\n    \"type\": \"oauth\",\n    \"refresh\": \"other\"\n  }\n}"
    );
}

#[test]
fn invalid_auth_json_aborts_without_touching_any_file() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("auth.json"), "{not json").unwrap();
    fs::write(temp.path().join("oauth.json"), r#"{"openai":{"refresh":"r"}}"#).unwrap();

    assert!(migrate_auth_to_auth_json_in(temp.path()).is_empty());
    assert_eq!(read(&temp.path().join("auth.json")), "{not json");
    assert!(temp.path().join("oauth.json").exists());
}

#[test]
fn no_providers_does_not_create_auth_json() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("settings.json"), r#"{"theme":"dark"}"#).unwrap();

    assert!(migrate_auth_to_auth_json_in(temp.path()).is_empty());
    assert!(!temp.path().join("auth.json").exists());
}

#[test]
fn session_directory_encoding_matches_session_manager() {
    assert_eq!(
        encode_session_directory_name(&JsString::from("/home/user/project")),
        "--home-user-project--"
    );
    assert_eq!(
        encode_session_directory_name(&JsString::from(r"C:\work:dir")),
        "--C--work-dir--"
    );
    assert_eq!(
        encode_session_directory_name(&JsString::from("relative")),
        "--relative--"
    );
}

#[test]
fn moves_agent_root_sessions_into_encoded_directories() {
    let temp = TempDir::new().unwrap();
    let header = json!({"type": "session", "cwd": "/work/app", "version": 3});
    fs::write(
        temp.path().join("2026-01-01.jsonl"),
        format!("{header}\n{{\"type\":\"message\"}}\n"),
    )
    .unwrap();

    migrate_sessions_from_agent_root_in(temp.path());

    let moved = temp.path().join("sessions/--work-app--/2026-01-01.jsonl");
    assert!(moved.exists());
    assert!(read(&moved).starts_with(&header.to_string()));
    assert!(!temp.path().join("2026-01-01.jsonl").exists());
}

#[test]
fn skips_non_session_headers_and_existing_targets() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("plain.jsonl"), "{\"type\":\"message\"}\n").unwrap();
    fs::write(temp.path().join("blank.jsonl"), "\n{\"type\":\"session\"}\n").unwrap();
    fs::write(temp.path().join("bad.jsonl"), "not json\n").unwrap();
    fs::write(temp.path().join("nested.jsonl"), "").unwrap();
    let header = json!({"type": "session", "cwd": "/dup"});
    fs::write(temp.path().join("dup.jsonl"), format!("{header}\n")).unwrap();
    let target_dir = temp.path().join("sessions/--dup--");
    fs::create_dir_all(&target_dir).unwrap();
    fs::write(target_dir.join("dup.jsonl"), "existing").unwrap();

    migrate_sessions_from_agent_root_in(temp.path());

    assert!(temp.path().join("plain.jsonl").exists());
    assert!(temp.path().join("blank.jsonl").exists());
    assert!(temp.path().join("bad.jsonl").exists());
    assert!(temp.path().join("nested.jsonl").exists());
    assert!(temp.path().join("dup.jsonl").exists());
    assert_eq!(read(&target_dir.join("dup.jsonl")), "existing");
}

#[test]
fn moves_managed_binaries_and_drops_duplicates() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(agent_dir.join("tools")).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(agent_dir.join("tools/fd"), "fd-binary").unwrap();
    fs::write(agent_dir.join("tools/rg"), "stale").unwrap();
    fs::write(bin_dir.join("rg"), "existing").unwrap();
    fs::write(agent_dir.join("tools/other.txt"), "keep").unwrap();

    migrate_tools_to_bin_in(&agent_dir, &bin_dir);

    assert_eq!(read(&bin_dir.join("fd")), "fd-binary");
    assert!(!agent_dir.join("tools/fd").exists());
    assert!(!agent_dir.join("tools/rg").exists());
    assert_eq!(read(&bin_dir.join("rg")), "existing");
    assert!(agent_dir.join("tools/other.txt").exists());
}

#[test]
fn keybindings_file_is_rewritten_only_when_migration_applies() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("keybindings.json");
    fs::write(&path, r#"{"submit":"enter","cursorUp":"k"}"#).unwrap();
    run_migrations_in(
        temp.path(),
        &temp.path().join("bin"),
        &JsString::from(temp.path().to_str().unwrap()),
    );

    assert_eq!(
        read(&path),
        "{\n  \"tui.editor.cursorUp\": \"k\",\n  \"tui.input.submit\": \"enter\"\n}\n"
    );
}

#[test]
fn keybindings_file_is_untouched_for_malformed_or_unchanged_content() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("keybindings.json");
    fs::write(&path, "[1,2]").unwrap();
    run_migrations_in(
        temp.path(),
        &temp.path().join("bin"),
        &JsString::from(temp.path().to_str().unwrap()),
    );
    assert_eq!(read(&path), "[1,2]");

    fs::write(&path, r#"{"tui.input.submit":"enter"}"#).unwrap();
    run_migrations_in(
        temp.path(),
        &temp.path().join("bin"),
        &JsString::from(temp.path().to_str().unwrap()),
    );
    assert_eq!(read(&path), r#"{"tui.input.submit":"enter"}"#);
}

#[test]
fn reports_deprecated_hook_and_tool_directories() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let project_dir = temp.path().join(".pi");
    fs::create_dir_all(agent_dir.join("hooks")).unwrap();
    fs::create_dir_all(agent_dir.join("tools")).unwrap();
    fs::write(agent_dir.join("tools/fd"), "binary").unwrap();
    fs::write(agent_dir.join("tools/.DS_Store"), "hidden").unwrap();
    fs::create_dir_all(project_dir.join("tools")).unwrap();
    fs::write(project_dir.join("tools/custom.ts"), "tool").unwrap();

    let result = run_migrations_in(
        &agent_dir,
        &temp.path().join("bin"),
        &JsString::from(temp.path().to_str().unwrap()),
    );

    assert_eq!(
        result.deprecation_warnings,
        vec![
            "Global hooks/ directory found. Hooks have been renamed to extensions.".to_owned(),
            "Project tools/ directory contains custom tools. Custom tools have been merged into extensions.".to_owned(),
        ]
    );
}

#[test]
fn migrates_commands_directories_for_agent_and_project() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    fs::create_dir_all(agent_dir.join("commands")).unwrap();
    fs::write(agent_dir.join("commands/a.md"), "a").unwrap();
    fs::create_dir_all(temp.path().join(".pi/commands")).unwrap();

    run_migrations_in(
        &agent_dir,
        &temp.path().join("bin"),
        &JsString::from(temp.path().to_str().unwrap()),
    );

    assert!(agent_dir.join("prompts/a.md").exists());
    assert!(temp.path().join(".pi/prompts").is_dir());
    assert!(!temp.path().join(".pi/commands").exists());
}
