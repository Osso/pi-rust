//! Differential coverage for `modes/rpc/rpc-mode.ts`.
//!
//! Drives the real pinned TypeScript RPC mode and the Rust port over stdin/stdout
//! with the faux provider and compares every response/event line byte-for-byte
//! after substituting ids, timestamps, durations, and temp paths.

#[path = "support/mode_harness.rs"]
mod mode_harness;

use std::path::Path;

use mode_harness::{ModeHarness, Normalizer, create_package_dir, read_source_lock, verify_source_head};
use serde_json::{Value, json};
use tempfile::TempDir;

const RPC_MODE_MODULE: &str = "packages/coding-agent/src/modes/rpc/rpc-mode.ts";
const RESPONSES: &str = r#"[{"text":"Hello there","endTurn":"greeted","toolId":"call-1"}]"#;

struct Session {
    ts: ModeHarness,
    rust: ModeHarness,
    normalizer: Normalizer,
}

impl Session {
    fn start() -> (Self, TempDir) {
        Self::start_with(&[])
    }

    fn start_with(extra_env: &[(&'static str, &'static str)]) -> (Self, TempDir) {
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
                ("PI_HARNESS_RESPONSES", RESPONSES.to_owned()),
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
        for (name, value) in extra_env {
            ts_env.push((name, (*value).to_owned()));
            rust_env.push((name, (*value).to_owned()));
        }

        let ts = ModeHarness::spawn_typescript(&lock, RPC_MODE_MODULE, &ts_env, Path::new(&cwd_string));
        let rust = ModeHarness::spawn_rust(&rust_env, Path::new(&cwd_string));
        (Self { ts, rust, normalizer }, root)
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
        assert_eq!(self.normalizer.normalize(expected), self.normalizer.normalize(actual));
    }
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
    let ts = session.ts.wait_for(|line| line.contains("\"type\":\"agent_settled\""));
    let rust = session
        .rust
        .wait_for(|line| line.contains("\"type\":\"agent_settled\""));
    session.assert_same(&ts, &rust);

    for command in [
        json!({"id": "b1", "type": "bash", "command": "echo hello"}),
        json!({"id": "b2", "type": "abort_bash"}),
        json!({"id": "sn2", "type": "set_session_name", "name": "demo"}),
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

    let normalizer = session.normalizer.clone();
    let (_, ts_code, ts_stderr) = session.ts.finish();
    let (_, rust_code, rust_stderr) = session.rust.finish();
    assert_eq!(ts_code, rust_code, "exit codes differ");
    assert_eq!(normalizer.normalize(&ts_stderr), normalizer.normalize(&rust_stderr));
}

#[test]
fn rpc_mode_continues_an_interrupted_restored_session() {
    let (mut session, _root) = Session::start_with(&[("PI_HARNESS_RESUME", "1")]);

    let ts = session.ts.wait_for(|line| line.contains("\"type\":\"agent_settled\""));
    let rust = session
        .rust
        .wait_for(|line| line.contains("\"type\":\"agent_settled\""));
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
