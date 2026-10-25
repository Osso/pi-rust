#[path = "support/oracle.rs"]
mod oracle;

use serde_json::json;
use std::path::Path;
use std::process::{Command, Output};

fn run_export(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(args)
        .current_dir(root)
        .env("HOME", root.join("home"))
        .env("PI_CODING_AGENT_DIR", root.join("agent"))
        .env("PI_CODING_AGENT_STATE_DIR", root.join("state"))
        .env("PI_OFFLINE", "1")
        .env("NO_COLOR", "1")
        .env_remove("PI_PACKAGE_DIR")
        .env_remove("PI_CODING_AGENT_SESSION_DIR")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run terminal export")
}

#[test]
fn terminal_export_rejects_incompatible_flags_before_runtime_startup() {
    let root = tempfile::tempdir().unwrap();
    let output = run_export(
        root.path(),
        &["--export-terminal", "session.jsonl", "--export-columns", "80", "--help"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Error: --export-terminal accepts only --export-columns, --session-dir, and --offline\n"
    );
    assert_eq!(
        std::fs::read_dir(root.path()).unwrap().count(),
        0,
        "read-only path must not create settings, sessions or control DB"
    );
}

#[test]
fn terminal_export_cli_matches_frozen_saved_history_without_writes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("history.jsonl");
    let header = json!({"type":"session","version":3,"id":"11111111-1111-4111-8111-111111111111","timestamp":"2026-01-01T00:00:00.000Z","cwd":root.path()});
    let messages = [
        json!({"role":"user","content":"Hello\r\n**world**\u{1b}]52;c;bad\u{7}\u{1b}[31m red\u{1b}[0m\u{0}\u{7f}","timestamp":1}),
        json!({"role":"assistant","content":[{"type":"text","text":"Answer with `code`."}],"api":"faux","provider":"faux","model":"model","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"totalTokens":2,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":2}),
        json!({"role":"bashExecution","command":"echo hello","output":"hello\r\n","exitCode":0,"cancelled":false,"truncated":false,"timestamp":3}),
        json!({"role":"custom","customType":"notice","content":"Saved **notice**","display":true,"timestamp":4}),
        json!({"role":"custom","customType":"hidden","content":"SECRET HIDDEN","display":false,"timestamp":5}),
    ];
    let mut lines = vec![header.to_string()];
    for (index, message) in messages.into_iter().enumerate() {
        lines.push(json!({"type":"message","id":format!("m{index}"),"parentId":index.checked_sub(1).map(|i|format!("m{i}")),"timestamp":"2026-01-01T00:00:00.000Z","message":message}).to_string());
    }
    let original = lines.join("\n") + "\n";
    std::fs::write(&path, &original).unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "export-terminal-oracle.ts",
        "packages/coding-agent/src/core/export-terminal.ts",
    );
    for columns in [20, 80] {
        let expected = oracle.request(&json!({"id":columns,"path":path,"columns":columns}));
        assert_eq!(expected["ok"], true, "{expected}");
        let output = run_export(
            root.path(),
            &[
                "--export-terminal",
                path.to_str().unwrap(),
                "--export-columns",
                &columns.to_string(),
                "--offline",
            ],
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            expected["output"].as_str().unwrap()
        );
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}
