use crate::shared_oracle as oracle;

use serde_json::json;
use std::path::Path;
use std::process::{Command, Output};

fn run_export(root: &Path, args: &[&str]) -> Output {
    run_export_with_session_dir(root, args, None)
}

fn run_export_with_session_dir(root: &Path, args: &[&str], session_dir: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pi"));
    command
        .args(args)
        .current_dir(root)
        .env("HOME", root.join("home"))
        .env("PI_CODING_AGENT_DIR", root.join("agent"))
        .env("PI_CODING_AGENT_STATE_DIR", root.join("state"))
        .env("PI_OFFLINE", "1")
        .env("NO_COLOR", "1")
        .env_remove("PI_PACKAGE_DIR")
        .env_remove("PI_CODING_AGENT_SESSION_DIR")
        .stdin(std::process::Stdio::null());
    if let Some(session_dir) = session_dir {
        command.env("PI_CODING_AGENT_SESSION_DIR", session_dir);
    }
    command.output().expect("run terminal export")
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
    let mut messages = messages.to_vec();
    messages.extend([
        json!({"role":"user","content":[{"type":"image","data":"AA==","mimeType":"image/png"}],"timestamp":6}),
        json!({"role":"user","content":"<skill name=\"demo\" location=\"/tmp/SKILL.md\">\nFollow instructions.\n</skill>\n\nApply this skill.","timestamp":7}),
        assistant("toolUse", vec![json!({"type":"thinking","thinking":"A saved thought"}), json!({"type":"toolCall","id":"t1","name":"custom-tool","arguments":{"query":"saved\u{1b}[31m query"}})]),
        json!({"role":"toolResult","toolCallId":"t1","toolName":"custom-tool","content":[{"type":"text","text":"saved result"},{"type":"image","data":"AA==","mimeType":"image/png"}],"isError":false,"timestamp":9}),
        json!({"role":"toolResult","toolCallId":"orphan","toolName":"missing","content":[{"type":"text","text":"orphan result"}],"isError":true,"timestamp":10}),
        assistant("aborted", vec![json!({"type":"toolCall","id":"a1","name":"custom-tool","arguments":{}})]),
        assistant("error", vec![json!({"type":"toolCall","id":"e1","name":"custom-tool","arguments":{}})]),
        json!({"role":"branchSummary","summary":"A saved branch summary","fromId":"old","timestamp":11}),
        json!({"role":"compactionSummary","summary":"A saved compacted summary","tokensBefore":1234,"timestamp":12}),
        json!({"role":"bashExecution","command":"cancelled","output":"saved truncated output","exitCode":2,"cancelled":true,"truncated":true,"fullOutputPath":"/tmp/full.log","excludeFromContext":true,"timestamp":13}),
    ]);
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
    for (columns, theme) in [(20, "dark"), (80, "dark"), (40, "light")] {
        std::fs::create_dir_all(root.path().join("agent")).unwrap();
        let settings = json!({"theme":theme}).to_string();
        std::fs::write(root.path().join("agent/settings.json"), &settings).unwrap();
        let expected = oracle.request(&json!({"id":columns,"path":path,"columns":columns,"theme":theme}));
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
        assert_eq!(
            std::fs::read_to_string(root.path().join("agent/settings.json")).unwrap(),
            settings
        );
        assert_eq!(std::fs::read_dir(root.path().join("agent")).unwrap().count(), 1);
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
}

#[test]
fn terminal_export_file_errors_match_frozen_and_width_errors_fail_before_io() {
    use pi_coding_agent_interactive::export_terminal::export_session_to_terminal;
    use pi_js_compat::js_json::JsString;
    let root = tempfile::tempdir().unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "export-terminal-oracle.ts",
        "packages/coding-agent/src/core/export-terminal.ts",
    );
    for (id, contents) in [None, Some("not a session\n")].into_iter().enumerate() {
        let path = root.path().join(format!("bad{id}.jsonl"));
        if let Some(contents) = contents {
            std::fs::write(&path, contents).unwrap();
        }
        let expected = oracle.request(&json!({"id":id,"path":path,"columns":80}));
        assert_eq!(expected["ok"], false);
        let output = run_export(
            root.path(),
            &["--export-terminal", path.to_str().unwrap(), "--export-columns", "80"],
        );
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            format!("Error: {}\n", expected["error"]["message"].as_str().unwrap())
        );
    }
    let path = JsString::from(root.path().join("absent.jsonl").to_str().unwrap());
    for columns in [0.0, -1.0, 1.5] {
        let expected = oracle.request(&json!({"id":columns,"path":path,"columns":columns}));
        let actual = export_session_to_terminal(&path, columns).unwrap_err();
        assert_eq!(
            actual.message,
            JsString::from(expected["error"]["message"].as_str().unwrap())
        );
    }
}

fn assistant(stop_reason: &str, content: Vec<serde_json::Value>) -> serde_json::Value {
    json!({"role":"assistant","content":content,"api":"faux","provider":"faux","model":"model","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"totalTokens":2,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":stop_reason,"timestamp":8})
}

#[test]
fn terminal_export_id_directory_precedence_and_compacted_projection_match_frozen() {
    let root = tempfile::tempdir().unwrap();
    let id = "12345678-1234-4123-8123-123456789abc";
    let mut oracle = oracle::TsOracle::spawn(
        "export-terminal-oracle.ts",
        "packages/coding-agent/src/core/export-terminal.ts",
    );
    let mut paths = Vec::new();
    for name in ["settings", "environment", "argument"] {
        let dir = root.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("2026-01-01T00-00-00-000Z_{id}.jsonl"));
        let header =
            json!({"type":"session","version":3,"id":id,"timestamp":"2026-01-01T00:00:00.000Z","cwd":root.path()});
        let entries = [
            json!({"type":"message","id":"old","parentId":null,"timestamp":"2026-01-01T00:00:00.000Z","message":{"role":"user","content":"BEFORE COMPACTION MUST NOT SHOW","timestamp":1}}),
            json!({"type":"message","id":"kept","parentId":"old","timestamp":"2026-01-01T00:00:01.000Z","message":{"role":"user","content":format!("Retained {name} history"),"timestamp":2}}),
            json!({"type":"compaction","id":"compact","parentId":"kept","timestamp":"2026-01-01T00:00:02.000Z","summary":format!("Saved {name} summary"),"firstKeptEntryId":"kept","tokensBefore":1234}),
            json!({"type":"message","id":"after","parentId":"compact","timestamp":"2026-01-01T00:00:03.000Z","message":{"role":"user","content":format!("After {name} compaction"),"timestamp":4}}),
        ];
        let original = std::iter::once(header)
            .chain(entries)
            .map(|entry| entry.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(&path, &original).unwrap();
        paths.push((dir, path, original));
    }
    let agent = root.path().join("agent");
    std::fs::create_dir(&agent).unwrap();
    let settings = json!({"sessionDir":paths[0].0,"theme":"dark"}).to_string();
    std::fs::write(agent.join("settings.json"), &settings).unwrap();
    for (index, env_dir, arg_dir) in [
        (0, None, None),
        (1, Some(paths[1].0.as_path()), None),
        (2, Some(paths[1].0.as_path()), Some(paths[2].0.as_path())),
    ] {
        let expected = oracle.request(&json!({"id":index,"path":paths[index].1,"columns":60}));
        assert_eq!(expected["ok"], true, "{expected}");
        let mut args = vec!["--export-terminal", "12345678", "--export-columns", "60"];
        if let Some(dir) = arg_dir {
            args.extend(["--session-dir", dir.to_str().unwrap()]);
        }
        let actual = run_export_with_session_dir(root.path(), &args, env_dir);
        assert_eq!(
            actual.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert!(actual.stderr.is_empty());
        let actual = String::from_utf8(actual.stdout).unwrap();
        assert_eq!(actual, expected["output"].as_str().unwrap());
        assert!(!actual.contains("BEFORE COMPACTION"));
        assert!(actual.contains("Retained"));
    }
    assert_eq!(std::fs::read_to_string(agent.join("settings.json")).unwrap(), settings);
    assert_eq!(std::fs::read_dir(&agent).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 4);
    for (dir, path, original) in paths {
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
        assert_eq!(std::fs::read_dir(dir).unwrap().count(), 1);
    }
}

#[test]
fn terminal_export_missing_id_and_external_alias_do_not_create_state() {
    let root = tempfile::tempdir().unwrap();
    for (session, code, message) in [
        (
            "no-such-session",
            4,
            "Error: No session found matching 'no-such-session'\n",
        ),
        (
            "claude/abc",
            1,
            "Error: External session aliases are not supported by read-only terminal export\n",
        ),
    ] {
        let output = run_export(root.path(), &["--export-terminal", session, "--export-columns", "80"]);
        assert_eq!(output.status.code(), Some(code), "{session}");
        assert!(output.stdout.is_empty());
        assert_eq!(String::from_utf8(output.stderr).unwrap(), message);
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
