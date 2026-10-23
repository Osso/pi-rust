//! Differential coverage for `modes/rpc/rpc-mode.ts`.
//!
//! Drives the real pinned TypeScript RPC mode and the Rust port over stdin/stdout
//! with the faux provider and compares every response/event line byte-for-byte
//! after substituting ids, timestamps, durations, and temp paths.

#[path = "support/mode_harness.rs"]
mod mode_harness;

use std::path::{Path, PathBuf};

use mode_harness::{ModeHarness, Normalizer, create_package_dir, read_source_lock, verify_source_head};
use serde_json::{Value, json};
use tempfile::TempDir;

const RPC_MODE_MODULE: &str = "packages/coding-agent/src/modes/rpc/rpc-mode.ts";
const RESPONSES: &str = r#"[{"text":"Hello there","endTurn":"greeted","toolId":"call-1"}]"#;

struct Session {
    ts: ModeHarness,
    rust: ModeHarness,
    normalizer: Normalizer,
    context_logs: [PathBuf; 2],
}

impl Session {
    fn start() -> (Self, TempDir) {
        Self::start_with(&[])
    }

    fn start_with(extra_env: &[(&'static str, &'static str)]) -> (Self, TempDir) {
        Self::start_with_responses(extra_env, RESPONSES)
    }

    fn start_with_responses(extra_env: &[(&'static str, &'static str)], responses: &str) -> (Self, TempDir) {
        let root = tempfile::tempdir().expect("temp dir");
        let cwd = root.path().join("cwd");
        for directory in [
            cwd.clone(),
            root.path().join("ts/agent"),
            root.path().join("ts/sessions"),
            root.path().join("ts/state"),
            root.path().join("rust/agent"),
            root.path().join("rust/sessions"),
            root.path().join("rust/state"),
        ] {
            std::fs::create_dir_all(&directory).expect("fixture directory");
        }
        let lock = read_source_lock();
        verify_source_head(&lock);
        let ts_package = lock.worktree.join("packages/coding-agent");
        let package_dir = create_package_dir(ts_package.display().to_string().len());
        let cwd_string = cwd.display().to_string();
        let normalizer = Normalizer::new()
            .path(&cwd_string, "<cwd>")
            .path(&ts_package.display().to_string(), "<pkg>")
            .path(&package_dir.display().to_string(), "<pkg>")
            .path(&root.path().display().to_string(), "<tmp>");
        let common = |agent: String, sessions: String, state: String| {
            vec![
                ("PI_HARNESS_MODE", "rpc".to_owned()),
                ("PI_HARNESS_CWD", cwd_string.clone()),
                ("PI_HARNESS_RESPONSES", responses.to_owned()),
                ("PI_HARNESS_AGENT_DIR", agent),
                ("PI_HARNESS_SESSION_DIR", sessions),
                ("PI_CODING_AGENT_STATE_DIR", state),
            ]
        };
        let mut ts_env = common(
            root.path().join("ts/agent").display().to_string(),
            root.path().join("ts/sessions").display().to_string(),
            root.path().join("ts/state").display().to_string(),
        );
        let mut rust_env = common(
            root.path().join("rust/agent").display().to_string(),
            root.path().join("rust/sessions").display().to_string(),
            root.path().join("rust/state").display().to_string(),
        );
        rust_env.push(("PI_PACKAGE_DIR", package_dir.display().to_string()));
        let context_logs = [
            root.path().join("source-context.jsonl"),
            root.path().join("native-context.jsonl"),
        ];
        ts_env.push(("PI_HARNESS_CONTEXT_LOG", context_logs[0].display().to_string()));
        rust_env.push(("PI_HARNESS_CONTEXT_LOG", context_logs[1].display().to_string()));
        for (name, value) in extra_env {
            ts_env.push((name, (*value).to_owned()));
            rust_env.push((name, (*value).to_owned()));
        }

        let ts = ModeHarness::spawn_typescript(&lock, RPC_MODE_MODULE, &ts_env, Path::new(&cwd_string));
        let rust = ModeHarness::spawn_rust(&rust_env, Path::new(&cwd_string));
        (
            Self {
                ts,
                rust,
                normalizer,
                context_logs,
            },
            root,
        )
    }

    fn send_both(&mut self, line: &str) {
        self.ts.send(line);
        self.rust.send(line);
    }

    fn response(&mut self, id: &str) -> (String, String) {
        let ts = self.ts.wait_for_id(id);
        let rust = self.rust.wait_for_id(id);
        (ts, rust)
    }

    fn command(&mut self, command: Value) -> (String, String) {
        let id = command["id"].as_str().expect("command id").to_owned();
        self.send_both(&command.to_string());
        self.response(&id)
    }

    fn assert_same(&self, expected: &str, actual: &str) {
        assert_eq!(
            self.normalizer.normalize(expected),
            self.normalizer.normalize(actual),
            "captured contexts source={} native={}",
            std::fs::read_to_string(&self.context_logs[0])
                .unwrap_or_else(|error| format!("capture unavailable: {error}")),
            std::fs::read_to_string(&self.context_logs[1])
                .unwrap_or_else(|error| format!("capture unavailable: {error}"))
        );
    }
}

#[test]
fn done_error_matches_frozen_rpc_session_events() {
    let (mut session, _root) =
        Session::start_with_responses(&[("PI_HARNESS_DONE_ERROR", "1")], r#"[{"stopReason":"error"}]"#);
    let (ts, rust) = session.command(json!({"id":"retry","type":"set_auto_retry","enabled":false}));
    session.assert_same(&ts, &rust);
    session.send_both(r#"{"id":"prompt","type":"prompt","message":"hello"}"#);
    let (ts, rust) = session.response("prompt");
    session.assert_same(&ts, &rust);
    let agent_end = |line: &str| line.contains("\"type\":\"agent_end\"");
    let ts = session.ts.wait_for(agent_end);
    let rust = session.rust.wait_for(agent_end);
    session.assert_same(&ts, &rust);
    let (ts, rust) = session.command(json!({"id":"messages","type":"get_messages"}));
    session.assert_same(&ts, &rust);
    assert!(rust.contains("\"stopReason\":\"error\""));
    assert!(!rust.contains("An unknown error occurred"));
    let (source, source_exit, source_stderr) = session.ts.finish();
    let (native, native_exit, native_stderr) = session.rust.finish();
    assert_eq!(source_exit, native_exit);
    assert_eq!(
        session.normalizer.normalize(&source_stderr),
        session.normalizer.normalize(&native_stderr)
    );
    let normalize = |lines: Vec<String>| {
        lines
            .iter()
            .map(|line| session.normalizer.normalize(line))
            .collect::<Vec<_>>()
    };
    assert_eq!(normalize(source), normalize(native));
}

#[test]
fn rpc_mode_matches_the_pinned_typescript_source() {
    let (mut session, _root) = Session::start();

    let static_commands = [
        json!({"id": "s1", "type": "get_state"}),
        json!({"id": "m1", "type": "get_available_models"}),
        json!({"id": "m2", "type": "set_model", "provider": "faux", "modelId": "faux-1"}),
        json!({"id": "t1", "type": "set_thinking_level", "level": "off"}),
        json!({"id": "t2", "type": "cycle_thinking_level"}),
        json!({"id": "t3", "type": "get_available_thinking_levels"}),
        json!({"id": "q1", "type": "set_steering_mode", "mode": "all"}),
        json!({"id": "q2", "type": "set_follow_up_mode", "mode": "one-at-a-time"}),
        json!({"id": "c1", "type": "set_auto_compaction", "enabled": false}),
        json!({"id": "r1", "type": "set_auto_retry", "enabled": false}),
        json!({"id": "r2", "type": "abort_retry"}),
        json!({"id": "st1", "type": "get_session_stats"}),
        json!({"id": "msg1", "type": "get_messages"}),
        json!({"id": "e1", "type": "get_entries"}),
        json!({"id": "tr1", "type": "get_tree"}),
        json!({"id": "la1", "type": "get_last_assistant_text"}),
        json!({"id": "fm1", "type": "get_fork_messages"}),
        json!({"id": "cmd1", "type": "get_commands"}),
        json!({"id": "sn1", "type": "set_session_name", "name": "   "}),
    ];
    for command in static_commands {
        let (ts, rust) = session.command(command);
        session.assert_same(&ts, &rust);
    }

    session.send_both(r#"{"id":"p1","type":"prompt","message":"hello"}"#);
    let (ts, rust) = session.response("p1");
    session.assert_same(&ts, &rust);
    let ts = session.ts.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    let rust = session.rust.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    session.assert_same(&ts, &rust);

    for command in [
        json!({"id": "b1", "type": "bash", "command": "echo hello"}),
        json!({"id": "b2", "type": "abort_bash"}),
        json!({"id": "s2", "type": "get_state"}),
        json!({"id": "la2", "type": "get_last_assistant_text"}),
        json!({"id": "st2", "type": "get_session_stats"}),
        json!({"id": "h1", "type": "export_html"}),
        json!({"id": "u1", "type": "bogus"}),
    ] {
        let (ts, rust) = session.command(command);
        session.assert_same(&ts, &rust);
    }

    session.send_both("{oops");
    let ts = session.ts.wait_for(|line| line.contains("\"command\":\"parse\""));
    let rust = session.rust.wait_for(|line| line.contains("\"command\":\"parse\""));
    session.assert_same(&ts, &rust);

    let (ts_entries, rust_entries) = session.command(json!({"id": "e2", "type": "get_entries"}));
    session.assert_same(&ts_entries, &rust_entries);
    let (ts_tree, rust_tree) = session.command(json!({"id": "tr2", "type": "get_tree"}));
    session.assert_same(&ts_tree, &rust_tree);

    let ts_entry_id = first_entry_id(&ts_entries);
    let rust_entry_id = first_entry_id(&rust_entries);
    session
        .ts
        .send(&json!({"id": "e3", "type": "get_entries", "since": ts_entry_id}).to_string());
    session
        .rust
        .send(&json!({"id": "e3", "type": "get_entries", "since": rust_entry_id}).to_string());
    let (ts, rust) = session.response("e3");
    session.assert_same(&ts, &rust);

    session
        .ts
        .send(&json!({"id": "e4", "type": "get_entries", "since": "nonexistent"}).to_string());
    session
        .rust
        .send(&json!({"id": "e4", "type": "get_entries", "since": "nonexistent"}).to_string());
    let (ts, rust) = session.response("e4");
    session.assert_same(&ts, &rust);

    let ts_fork_id = second_entry_id(&ts_entries);
    let rust_fork_id = second_entry_id(&rust_entries);
    session
        .ts
        .send(&json!({"id": "f1", "type": "fork", "entryId": ts_fork_id}).to_string());
    session
        .rust
        .send(&json!({"id": "f1", "type": "fork", "entryId": rust_fork_id}).to_string());
    let (ts, rust) = session.response("f1");
    session.assert_same(&ts, &rust);

    for command in [
        json!({"id": "s3", "type": "get_state"}),
        json!({"id": "cl1", "type": "clone"}),
        json!({"id": "n1", "type": "new_session"}),
        json!({"id": "s4", "type": "get_state"}),
        json!({"id": "la3", "type": "get_last_assistant_text"}),
    ] {
        let (ts, rust) = session.command(command);
        session.assert_same(&ts, &rust);
    }

    for command in [
        json!({"id":"sn2", "type":"set_session_name", "name":"demo"}),
        json!({"id":"named-state", "type":"get_state"}),
    ] {
        let (ts, rust) = session.command(command);
        session.assert_same(&ts, &rust);
    }

    let normalizer = session.normalizer.clone();
    let (_, ts_code, ts_stderr) = session.ts.finish();
    let (_, rust_code, rust_stderr) = session.rust.finish();
    assert_eq!(ts_code, rust_code, "exit codes differ");
    assert_eq!(normalizer.normalize(&ts_stderr), normalizer.normalize(&rust_stderr));
}

#[test]
fn rpc_mode_reports_manual_compaction_failures() {
    let (mut session, _root) = Session::start_with_responses(
        &[],
        r#"[{"text":"Hello there","endTurn":"greeted","toolId":"call-1"},{"text":"Summary of the conversation."}]"#,
    );

    session.send_both(r#"{"id":"p1","type":"prompt","message":"hello"}"#);
    let (ts, rust) = session.response("p1");
    session.assert_same(&ts, &rust);
    let ts = session.ts.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    let rust = session.rust.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    session.assert_same(&ts, &rust);

    let (ts, rust) = session.command(json!({"id": "c1", "type": "compact"}));
    session.assert_same(&ts, &rust);
    let (ts, rust) = session.command(json!({"id": "s1", "type": "get_state"}));
    session.assert_same(&ts, &rust);

    let normalizer = session.normalizer.clone();
    let (_, ts_code, ts_stderr) = session.ts.finish();
    let (_, rust_code, rust_stderr) = session.rust.finish();
    assert_eq!(ts_code, rust_code);
    assert_eq!(normalizer.normalize(&ts_stderr), normalizer.normalize(&rust_stderr));
}

#[test]
fn rpc_mode_routes_extension_ui_dialogs() {
    let (mut session, _root) = Session::start_with(&[("PI_HARNESS_APPROVAL", "1")]);

    session.send_both(r#"{"id":"p1","type":"prompt","message":"hello"}"#);
    let (ts, rust) = session.response("p1");
    session.assert_same(&ts, &rust);

    let ts = session.ts.wait_for(|line| line.contains("\"method\":\"select\""));
    let rust = session.rust.wait_for(|line| line.contains("\"method\":\"select\""));
    session.assert_same(&ts, &rust);

    let ts_id = ui_request_id(&ts);
    let rust_id = ui_request_id(&rust);
    session
        .ts
        .send(&json!({"type": "extension_ui_response", "id": ts_id, "value": "Allow once"}).to_string());
    session
        .rust
        .send(&json!({"type": "extension_ui_response", "id": rust_id, "value": "Allow once"}).to_string());

    let ts = session.ts.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    let rust = session.rust.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    session.assert_same(&ts, &rust);

    let (ts, rust) = session.command(json!({"id": "s1", "type": "get_state"}));
    session.assert_same(&ts, &rust);

    let normalizer = session.normalizer.clone();
    let (_, ts_code, ts_stderr) = session.ts.finish();
    let (_, rust_code, rust_stderr) = session.rust.finish();
    assert_eq!(ts_code, rust_code);
    assert_eq!(normalizer.normalize(&ts_stderr), normalizer.normalize(&rust_stderr));
}

fn ui_request_id(line: &str) -> String {
    let value: Value = serde_json::from_str(line).expect("ui request json");
    value["id"].as_str().expect("ui request id").to_owned()
}

#[test]
fn rpc_mode_continues_an_interrupted_restored_session() {
    let (mut session, _root) = Session::start_with(&[("PI_HARNESS_RESUME", "1")]);

    let ts = session.ts.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    let rust = session.rust.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    session.assert_same(&ts, &rust);

    let (ts, rust) = session.command(json!({"id": "s1", "type": "get_state"}));
    session.assert_same(&ts, &rust);
    let (ts, rust) = session.command(json!({"id": "m1", "type": "get_messages"}));
    session.assert_same(&ts, &rust);

    let normalizer = session.normalizer.clone();
    let (_, ts_code, ts_stderr) = session.ts.finish();
    let (_, rust_code, rust_stderr) = session.rust.finish();
    assert_eq!(ts_code, rust_code);
    assert_eq!(normalizer.normalize(&ts_stderr), normalizer.normalize(&rust_stderr));
}

fn first_entry_id(response: &str) -> String {
    let value: Value = serde_json::from_str(response).expect("response json");
    value["data"]["entries"][0]["id"].as_str().expect("entry id").to_owned()
}

fn second_entry_id(response: &str) -> String {
    let value: Value = serde_json::from_str(response).expect("response json");
    value["data"]["entries"][1]["id"].as_str().expect("entry id").to_owned()
}
