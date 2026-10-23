//! Differential coverage for `modes/print-mode.ts`.
//!
//! Drives the real pinned TypeScript print mode and the Rust port over stdin/stdout
//! with the faux provider and compares stdout, stderr, and exit codes byte-for-byte
//! after substituting ids, timestamps, durations, and temp paths.

#[path = "support/mode_harness.rs"]
mod mode_harness;

use std::path::Path;

use mode_harness::{ModeHarness, Normalizer, create_package_dir, read_source_lock, verify_source_head};
use pi_js_compat::js_json::{JsString, JsValue, stringify_lossless};
use serde_json::json;
use tempfile::TempDir;

const PRINT_MODE_MODULE: &str = "packages/coding-agent/src/modes/print-mode.ts";

/// Collected stdout lines, exit code, and stderr.
type ModeOutcome = (Vec<String>, i32, String);

struct Fixture {
    _root: TempDir,
    cwd: String,
    ts_agent: String,
    ts_sessions: String,
    ts_state: String,
    rust_agent: String,
    rust_sessions: String,
    rust_state: String,
    package_dir: String,
    normalizer: Normalizer,
}

impl Fixture {
    fn new() -> Self {
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
        let source = read_source_lock();
        let ts_package = source.worktree.join("packages/coding-agent");
        let package_dir = create_package_dir(ts_package.display().to_string().len());
        let normalizer = Normalizer::new()
            .path(&cwd.display().to_string(), "<cwd>")
            .path(&ts_package.display().to_string(), "<pkg>")
            .path(&package_dir.display().to_string(), "<pkg>")
            .path(&root.path().display().to_string(), "<tmp>");
        Self {
            cwd: cwd.display().to_string(),
            ts_agent: root.path().join("ts/agent").display().to_string(),
            ts_sessions: root.path().join("ts/sessions").display().to_string(),
            ts_state: root.path().join("ts/state").display().to_string(),
            rust_agent: root.path().join("rust/agent").display().to_string(),
            rust_sessions: root.path().join("rust/sessions").display().to_string(),
            rust_state: root.path().join("rust/state").display().to_string(),
            package_dir: package_dir.display().to_string(),
            normalizer,
            _root: root,
        }
    }

    fn env(&self, mode: &str, prompts: &[&str], responses: impl Into<JsValue>) -> Vec<(&'static str, String)> {
        let responses = responses.into();
        let done_error = responses
            .as_array()
            .unwrap()
            .iter()
            .any(|response| response.get("doneError").and_then(JsValue::as_bool) == Some(true));
        let mut env = vec![
            ("PI_HARNESS_MODE", mode.to_owned()),
            ("PI_HARNESS_CWD", self.cwd.clone()),
            ("PI_HARNESS_PROMPTS", serde_json::to_string(prompts).expect("prompts")),
            ("PI_HARNESS_RESPONSES", stringify_lossless(&responses)),
        ];
        if done_error {
            env.push(("PI_HARNESS_DONE_ERROR", "1".into()));
        }
        if let Some(messages) = responses
            .as_array()
            .unwrap()
            .first()
            .and_then(|response| response.get("initialMessages"))
        {
            env.push(("PI_HARNESS_INITIAL_MESSAGES", stringify_lossless(messages)));
        }
        env
    }

    fn ts_env(&self, mode: &str, prompts: &[&str], responses: impl Into<JsValue>) -> Vec<(&'static str, String)> {
        let mut env = self.env(mode, prompts, responses);
        env.push(("PI_HARNESS_AGENT_DIR", self.ts_agent.clone()));
        env.push(("PI_HARNESS_SESSION_DIR", self.ts_sessions.clone()));
        env.push(("PI_CODING_AGENT_STATE_DIR", self.ts_state.clone()));
        env
    }

    fn rust_env(&self, mode: &str, prompts: &[&str], responses: impl Into<JsValue>) -> Vec<(&'static str, String)> {
        let mut env = self.env(mode, prompts, responses);
        env.push(("PI_HARNESS_AGENT_DIR", self.rust_agent.clone()));
        env.push(("PI_HARNESS_SESSION_DIR", self.rust_sessions.clone()));
        env.push(("PI_CODING_AGENT_STATE_DIR", self.rust_state.clone()));
        env.push(("PI_PACKAGE_DIR", self.package_dir.clone()));
        env
    }

    fn run(&self, mode: &str, prompts: &[&str], responses: impl Into<JsValue> + Clone) -> (ModeOutcome, ModeOutcome) {
        let lock = read_source_lock();
        verify_source_head(&lock);
        let ts = ModeHarness::spawn_typescript(
            &lock,
            PRINT_MODE_MODULE,
            &self.ts_env(mode, prompts, responses.clone()),
            Path::new(&self.cwd),
        )
        .finish();
        let rust = ModeHarness::spawn_rust(&self.rust_env(mode, prompts, responses), Path::new(&self.cwd)).finish();
        (ts, rust)
    }
}

fn compare(fixture: &Fixture, mode: &str, prompts: &[&str], responses: impl Into<JsValue> + Clone) {
    let ((ts_lines, ts_code, ts_stderr), (rust_lines, rust_code, rust_stderr)) = fixture.run(mode, prompts, responses);
    assert_eq!(
        ts_code, rust_code,
        "exit codes differ; source stderr: {ts_stderr}; native stderr: {rust_stderr}; source stdout: {ts_lines:?}; native stdout: {rust_lines:?}"
    );
    assert_eq!(
        fixture.normalizer.normalize(&ts_stderr),
        fixture.normalizer.normalize(&rust_stderr),
        "stderr differs"
    );
    assert_eq!(
        ts_lines.len(),
        rust_lines.len(),
        "line counts differ; source={ts_lines:?}; native={rust_lines:?}"
    );
    for (index, (expected, actual)) in ts_lines.iter().zip(rust_lines.iter()).enumerate() {
        assert_eq!(
            fixture.normalizer.normalize(expected),
            fixture.normalizer.normalize(actual),
            "line {index} differs"
        );
    }
}

#[test]
fn print_text_prints_the_final_assistant_text() {
    let fixture = Fixture::new();
    compare(
        &fixture,
        "print-text",
        &["hello"],
        json!([{"text": "Hello there", "endTurn": "greeted", "toolId": "call-1"}]),
    );
}

#[test]
fn print_text_respects_final_message_barriers() {
    let mut assistant = pi_ai::types::AssistantMessage::empty("faux", "faux", "faux-1", 0.0);
    assistant
        .content
        .push(pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
            "older",
        )));
    for suffix in [
        json!({"role":"user","content":"newer request","timestamp":1}),
        json!({"role":"toolResult","toolCallId":"x","toolName":"read","content":[],"isError":false,"timestamp":1}),
        json!({"role":"toolResult","toolCallId":"x","toolName":"end_turn","content":[],"isError":true,"timestamp":1}),
        serde_json::to_value(pi_ai::types::AssistantMessage::empty("faux", "faux", "faux-1", 1.0)).unwrap(),
    ] {
        let fixture = Fixture::new();
        compare(
            &fixture,
            "print-text",
            &[],
            json!([{"initialMessages":[assistant,suffix]}]),
        );
    }
}

#[test]
fn print_text_prints_the_end_turn_reason_when_no_text_was_produced() {
    let fixture = Fixture::new();
    compare(
        &fixture,
        "print-text",
        &["Read package.json"],
        json!([{"endTurn": "pi-monorepo", "toolId": "call-1"}]),
    );
}

#[test]
fn print_text_sends_follow_up_messages_and_prints_only_the_last_result() {
    let fixture = Fixture::new();
    compare(
        &fixture,
        "print-text",
        &["first", "second"],
        json!([
            {"text": "first answer", "endTurn": "first", "toolId": "call-1"},
            {"text": "second answer", "endTurn": "second", "toolId": "call-2"}
        ]),
    );
}

#[test]
fn done_error_matches_frozen_text_and_json_modes() {
    for mode in ["print-text", "print-json"] {
        let fixture = Fixture::new();
        compare(
            &fixture,
            mode,
            &["hello"],
            json!([{"stopReason":"error","doneError":true}]),
        );
    }
}

#[test]
fn print_text_reports_assistant_errors_and_exits_non_zero() {
    let fixture = Fixture::new();
    compare(
        &fixture,
        "print-text",
        &["hello"],
        json!([{"text": "", "stopReason": "error", "errorMessage": "provider failure"}]),
    );
}

#[test]
fn print_text_preserves_public_signed_narration_without_printing_private_thinking() {
    let signature = "Eg0KC0IJbmFycmF0aW9u";
    let raw = JsString::from(" public ") + JsString::from_utf16(vec![0xd800]) + " \n";
    let end = pi_js_compat::js_json!({"type":"toolCall","id":"narration-end","name":"end_turn","arguments":{"reason":"done"}});
    let public = pi_js_compat::js_json!({"type":"thinking","thinking":raw,"thinkingSignature":signature});
    for content in [
        pi_js_compat::js_json!([public.clone(), end.clone()]),
        pi_js_compat::js_json!([{"type":"thinking","thinking":"DO NOT PRINT","thinkingSignature":"private"}, public, {"type":"text","text":"tail"}, {"type":"text","text":""}, end]),
    ] {
        compare(
            &Fixture::new(),
            "print-text",
            &["hello"],
            pi_js_compat::js_json!([{"content":content}]),
        );
    }
}

#[test]
fn print_json_streams_every_event_and_the_session_header() {
    let fixture = Fixture::new();
    compare(
        &fixture,
        "print-json",
        &["hello"],
        json!([{"text": "Hello there", "endTurn": "greeted", "toolId": "call-1"}]),
    );
}
