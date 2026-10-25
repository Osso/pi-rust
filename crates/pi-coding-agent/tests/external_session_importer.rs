use std::fs;
use std::path::Path;

use pi_coding_agent::core::external_session_importer::{
    ExternalSessionImportOptions, import_external_session_alias, is_external_session_alias,
};
use pi_js_compat::js_json::JsString;
use tempfile::TempDir;

fn raw(path: &Path) -> JsString {
    JsString::from(path.to_str().expect("UTF-8 temp path"))
}

fn write_source(root: &Path, relative: &str, lines: &[&str]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write source");
}

fn options(home: &TempDir, sessions: &TempDir) -> ExternalSessionImportOptions {
    ExternalSessionImportOptions {
        home_dir: Some(raw(home.path())),
        session_dir: Some(raw(sessions.path())),
        fallback_cwd: Some("/fallback".into()),
    }
}

#[test]
fn claude_import_preserves_lone_surrogates_in_cwd_and_text() {
    let home = TempDir::new().expect("home");
    let sessions = TempDir::new().expect("sessions");
    write_source(
        home.path(),
        ".claude/projects/p/abc.jsonl",
        &[
            "not json",
            r#"{"sessionId":"abc","cwd":"/w\ud800","timestamp":"2026-01-02T03:04:05.000Z","message":{"role":"user","content":"\udc00hi"}}"#,
            r#"{"message":{"role":"assistant","content":[{"type":"output_text","text":"ok"},{"type":"image","text":"no"}]},"timestamp":"2026-01-02T03:04:06.000Z"}"#,
        ],
    );

    let imported = import_external_session_alias(&"claude/abc".into(), &options(&home, &sessions))
        .expect("import")
        .expect("found");

    assert_eq!(imported.id, "claude-abc");
    assert_eq!(
        imported.cwd.units(),
        JsString::from_utf16(vec![0x2f, 0x77, 0xd800]).units()
    );
    assert_eq!(imported.path, raw(&sessions.path().join("claude-abc.jsonl")));
    let written = fs::read_to_string(sessions.path().join("claude-abc.jsonl")).expect("written");
    let lines: Vec<&str> = written.trim_end().split('\n').collect();
    assert_eq!(lines.len(), 3, "{written}");
    assert!(lines[0].contains(r#""cwd":"/w\ud800""#), "{}", lines[0]);
    assert!(lines[1].contains(r#""text":"\udc00hi""#), "{}", lines[1]);
    assert!(
        lines[1].contains(r#""timestamp":"2026-01-02T03:04:05.000Z""#),
        "{}",
        lines[1]
    );
    assert!(lines[2].contains(r#""model":"imported-session""#), "{}", lines[2]);
    assert!(!lines[2].contains(r#""no""#), "{}", lines[2]);
}

#[test]
fn codex_file_name_id_is_the_allowed_run_before_the_suffix() {
    let home = TempDir::new().expect("home");
    let sessions = TempDir::new().expect("sessions");
    write_source(
        home.path(),
        ".codex/sessions/2026/rollout x1-0193.jsonl",
        &[r#"{"type":"response_item","payload":{"type":"message","role":"user","content":"hello"}}"#],
    );

    let imported = import_external_session_alias(&"codex/x1".into(), &options(&home, &sessions))
        .expect("import")
        .expect("found");

    assert_eq!(imported.id, "codex-x1-0193");
    assert_eq!(imported.cwd, "/fallback");
}

#[test]
fn file_names_replace_each_disallowed_utf16_unit() {
    let home = TempDir::new().expect("home");
    let sessions = TempDir::new().expect("sessions");
    write_source(
        home.path(),
        ".claude/projects/p/x.jsonl",
        &[r#"{"sessionId":"x\ud83d\ude00","message":{"role":"user","content":"hi"}}"#],
    );

    let imported = import_external_session_alias(&"claude/x".into(), &options(&home, &sessions))
        .expect("import")
        .expect("found");

    assert_eq!(imported.path, raw(&sessions.path().join("claude-x--.jsonl")));
}

#[test]
fn null_lines_and_invalid_timestamps_throw_like_the_source() {
    let home = TempDir::new().expect("home");
    let sessions = TempDir::new().expect("sessions");
    write_source(home.path(), ".claude/projects/p/n.jsonl", &["null"]);
    let error =
        import_external_session_alias(&"claude/n".into(), &options(&home, &sessions)).expect_err("null line throws");
    assert_eq!(error.message(), "Cannot read properties of null (reading 'sessionId')");

    write_source(
        home.path(),
        ".claude/projects/p/t.jsonl",
        &[r#"{"timestamp":"not a date","message":{"role":"user","content":"hi"}}"#],
    );
    let error = import_external_session_alias(&"claude/t".into(), &options(&home, &sessions))
        .expect_err("invalid timestamp throws");
    assert_eq!(error.message(), "Invalid time value");
}

#[test]
fn aliases_require_a_known_provider_and_a_single_segment_id() {
    assert!(is_external_session_alias(&"codex/abc".into()));
    assert!(!is_external_session_alias(&"codex/".into()));
    assert!(!is_external_session_alias(&"codex/a/b".into()));
    assert!(!is_external_session_alias(&"other/abc".into()));
}
