//! Differential coverage for `modes/print-mode.ts`.
//!
//! Drives the real pinned TypeScript print mode and the Rust port over stdin/stdout
//! with the faux provider and compares stdout, stderr, and exit codes byte-for-byte
//! after substituting ids, timestamps, durations, and temp paths.

use crate::mode_harness;

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
        if let Some(config) = responses
            .as_array()
            .unwrap()
            .first()
            .and_then(|response| response.get("printFixture"))
        {
            env.push(("PI_HARNESS_PRINT_FIXTURE", stringify_lossless(config)));
        }
        if let Some(images) = responses
            .as_array()
            .unwrap()
            .first()
            .and_then(|response| response.get("initialImages"))
        {
            env.push(("PI_HARNESS_INITIAL_IMAGES", stringify_lossless(images)));
        }
        env
    }

    fn ts_env(&self, mode: &str, prompts: &[&str], responses: impl Into<JsValue>) -> Vec<(&'static str, String)> {
        let mut env = self.env(mode, prompts, responses);
        env.push(("PI_HARNESS_AGENT_DIR", self.ts_agent.clone()));
        env.push(("PI_HARNESS_SESSION_DIR", self.ts_sessions.clone()));
        env.push(("PI_CODING_AGENT_STATE_DIR", self.ts_state.clone()));
        env.push(("PI_HARNESS_PRINT_ROOT", self.ts_state.clone()));
        env
    }

    fn rust_env(&self, mode: &str, prompts: &[&str], responses: impl Into<JsValue>) -> Vec<(&'static str, String)> {
        let mut env = self.env(mode, prompts, responses);
        env.push(("PI_HARNESS_AGENT_DIR", self.rust_agent.clone()));
        env.push(("PI_HARNESS_SESSION_DIR", self.rust_sessions.clone()));
        env.push(("PI_CODING_AGENT_STATE_DIR", self.rust_state.clone()));
        env.push(("PI_HARNESS_PRINT_ROOT", self.rust_state.clone()));
        env.push(("PI_PACKAGE_DIR", self.package_dir.clone()));
        env
    }

    fn run_reload_gate(&self, mode: &str, responses: impl Into<JsValue> + Clone) -> (ModeOutcome, ModeOutcome) {
        let lock = read_source_lock();
        verify_source_head(&lock);
        let ts = ModeHarness::spawn_typescript(
            &lock,
            PRINT_MODE_MODULE,
            &self.ts_env(mode, &["/print-reload-probe"], responses.clone()),
            Path::new(&self.cwd),
        );
        release_reload_when_pending(Path::new(&self.ts_state));
        let ts = ts.finish();
        let rust = ModeHarness::spawn_rust(
            &self.rust_env(mode, &["/print-reload-probe"], responses),
            Path::new(&self.cwd),
        );
        release_reload_when_pending(Path::new(&self.rust_state));
        (ts, rust.finish())
    }

    fn run_signalled(
        &self,
        mode: &str,
        responses: impl Into<JsValue> + Clone,
        signal: rustix::process::Signal,
    ) -> (ModeOutcome, ModeOutcome) {
        let lock = read_source_lock();
        verify_source_head(&lock);
        let ts = ModeHarness::spawn_typescript(
            &lock,
            PRINT_MODE_MODULE,
            &self.ts_env(mode, &["pending"], responses.clone()),
            Path::new(&self.cwd),
        );
        signal_when_ready(&ts, Path::new(&self.ts_state), signal);
        let ts = ts.finish();
        let rust = ModeHarness::spawn_rust(&self.rust_env(mode, &["pending"], responses), Path::new(&self.cwd));
        signal_when_ready(&rust, Path::new(&self.rust_state), signal);
        (ts, rust.finish())
    }

    fn run_repeated_signal(
        &self,
        first: rustix::process::Signal,
        second: rustix::process::Signal,
    ) -> (ModeOutcome, ModeOutcome) {
        let lock = read_source_lock();
        verify_source_head(&lock);
        let responses = json!([{"pending":true,"printFixture":{"pendingShutdown":true}}]);
        let ts = ModeHarness::spawn_typescript(
            &lock,
            PRINT_MODE_MODULE,
            &self.ts_env("print-text", &["pending"], responses.clone()),
            Path::new(&self.cwd),
        );
        signal_twice_when_shutdown_pending(&ts, Path::new(&self.ts_state), first, second);
        let source = ts.finish_with_exit_deadline(std::time::Duration::from_secs(5));
        let native = ModeHarness::spawn_rust(
            &self.rust_env("print-text", &["pending"], responses),
            Path::new(&self.cwd),
        );
        signal_twice_when_shutdown_pending(&native, Path::new(&self.rust_state), first, second);
        (
            source,
            native.finish_with_exit_deadline(std::time::Duration::from_secs(5)),
        )
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

fn release_reload_when_pending(root: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !root.join("print-reload-ready.json").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "reload dependency must become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::thread::sleep(std::time::Duration::from_millis(30));
    assert!(
        !root.join("print-reload-outcome.json").exists(),
        "reload caller must remain pending until its dependency rejects"
    );
    let entries = std::fs::read_to_string(root.join("print-reload-entries.jsonl")).unwrap();
    assert_eq!(
        entries.lines().count(),
        1,
        "runtime lifecycle gate must serialize reload dependencies"
    );
    std::fs::write(root.join("print-reload-release"), "release").unwrap();
}

fn signal_when_ready(harness: &ModeHarness, root: &Path, signal: rustix::process::Signal) {
    let ready = root.join("print-ready.json");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !ready.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "pending provider must become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(ready).unwrap()).unwrap();
    harness.signal_actual_process(u32::try_from(value["pid"].as_u64().unwrap()).unwrap(), signal);
}

fn signal_twice_when_shutdown_pending(
    harness: &ModeHarness,
    root: &Path,
    first: rustix::process::Signal,
    second: rustix::process::Signal,
) {
    signal_when_ready(harness, root, first);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !root.join("print-shutdown-ready").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "shutdown must enter the real extension handler"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    signal_when_ready(harness, root, second);
}

fn compare(fixture: &Fixture, mode: &str, prompts: &[&str], responses: impl Into<JsValue> + Clone) {
    let (source, native) = fixture.run(mode, prompts, responses);
    compare_outcomes(fixture, source, native);
}

fn compare_outcomes(fixture: &Fixture, source: ModeOutcome, native: ModeOutcome) {
    compare_outcomes_with_native_assistant_order(fixture, source, native, false);
}

fn compare_outcomes_with_native_assistant_order(
    fixture: &Fixture,
    source: ModeOutcome,
    native: ModeOutcome,
    native_assistant_order: bool,
) {
    let (ts_lines, ts_code, ts_stderr) = source;
    let (rust_lines, rust_code, rust_stderr) = native;
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
        let expected = fixture.normalizer.normalize(expected);
        let actual = fixture.normalizer.normalize(actual);
        if native_assistant_order && compare_native_assistant_event(&expected, &actual) {
            continue;
        }
        assert_eq!(expected, actual, "line {index} differs");
    }
}

fn compare_native_assistant_event(expected: &str, actual: &str) -> bool {
    let Ok(actual_event) = pi_js_compat::js_json::parse_lossless(actual) else {
        return false;
    };
    let eligible_event = actual_event
        .get("type")
        .and_then(JsValue::as_string)
        .is_some_and(|kind| kind == "message_start" || kind == "message_end");
    let Some(message) = actual_event.get("message").filter(|message| {
        message
            .get("role")
            .and_then(JsValue::as_string)
            .is_some_and(|role| role == "assistant")
    }) else {
        return false;
    };
    if !eligible_event {
        return false;
    }
    let expected_event = pi_js_compat::js_json::parse_lossless(expected).unwrap();
    // User approval2026-10-24 (AGENTS.md): native AssistantMessages in print/RPC
    // use fixed order; parsed values and every non-AssistantMessage order stay exact.
    assert_eq!(expected_event, actual_event, "complete parsed event values differ");
    let expected_fields = expected_event.as_object().unwrap();
    let actual_fields = actual_event.as_object().unwrap();
    assert_eq!(
        expected_fields.keys().collect::<Vec<_>>(),
        actual_fields.keys().collect::<Vec<_>>()
    );
    for (key, value) in expected_fields {
        if key == "message" {
            assert_native_assistant_fields(value, message);
        } else {
            assert_eq!(stringify_lossless(value), stringify_lossless(&actual_fields[key]));
        }
    }
    true
}

fn assert_native_assistant_fields(expected: &JsValue, actual: &JsValue) {
    let expected = expected.as_object().unwrap();
    let actual = actual.as_object().unwrap();
    let fixed_order = [
        "role",
        "content",
        "api",
        "provider",
        "model",
        "usage",
        "stopReason",
        "timestamp",
        "responseModel",
        "responseId",
        "imageGenerationResult",
        "diagnostics",
        "errorMessage",
    ];
    let present_order: Vec<_> = fixed_order
        .iter()
        .filter(|key| actual.contains_key(&JsString::from(**key)))
        .copied()
        .collect();
    let actual_order: Vec<_> = actual.keys().map(|key| key.to_utf8().unwrap()).collect();
    assert_eq!(actual_order, present_order, "native AssistantMessage fixed order");
    for (key, value) in expected {
        assert_eq!(
            stringify_lossless(value),
            stringify_lossless(&actual[key]),
            "AssistantMessage field {key:?} differs"
        );
    }
}

#[test]
#[should_panic(expected = "complete parsed event values differ")]
fn approved_assistant_order_still_rejects_changed_values() {
    compare_native_assistant_event(
        r#"{"type":"message_end","message":{"role":"assistant","timestamp":1,"errorMessage":"source"}}"#,
        r#"{"type":"message_end","message":{"role":"assistant","timestamp":1,"errorMessage":"changed"}}"#,
    );
}

#[test]
#[should_panic(expected = "native AssistantMessage fixed order")]
fn approved_assistant_order_still_rejects_nonfixed_native_order() {
    compare_native_assistant_event(
        r#"{"type":"message_end","message":{"role":"assistant","errorMessage":"error","timestamp":1}}"#,
        r#"{"type":"message_end","message":{"role":"assistant","errorMessage":"error","timestamp":1}}"#,
    );
}

#[test]
#[should_panic(expected = "AssistantMessage field")]
fn approved_assistant_order_still_rejects_changed_nested_order() {
    compare_native_assistant_event(
        r#"{"type":"message_end","message":{"role":"assistant","usage":{"input":1,"output":2},"timestamp":1}}"#,
        r#"{"type":"message_end","message":{"role":"assistant","usage":{"output":2,"input":1},"timestamp":1}}"#,
    );
}

#[test]
#[should_panic]
fn approved_assistant_order_still_rejects_changed_envelope_order() {
    compare_native_assistant_event(
        r#"{"type":"message_end","message":{"role":"assistant","timestamp":1}}"#,
        r#"{"message":{"role":"assistant","timestamp":1},"type":"message_end"}"#,
    );
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
fn print_rebind_preserves_unrelated_replacement_session_and_agent_listeners() {
    for mode in ["print-text", "print-json"] {
        let fixture = Fixture::new();
        let (source, native) = fixture.run(
            mode,
            &["/print-rebind-probe", "continue"],
            json!([{"text":"after-rebind","printFixture":{"rebindProbe":true}}]),
        );
        compare_outcomes_with_native_assistant_order(&fixture, source, native, mode == "print-json");
        for name in ["session", "agent"] {
            for root in [&fixture.ts_state, &fixture.rust_state] {
                let path = Path::new(root).join(format!("print-observer-{name}.json"));
                assert_eq!(
                    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!(
                        "replacement {name} listener must survive: {}: {error}",
                        path.display()
                    )),
                    "{\"observed\":true}"
                );
            }
        }
    }
}

#[test]
fn print_signals_are_registered_before_extension_start_binding() {
    for mode in ["print-text", "print-json"] {
        for (signal, code) in [("SIGTERM", 143), ("SIGHUP", 129)] {
            let fixture = Fixture::new();
            let responses = json!([{"printFixture":{"earlySignal":signal}}]);
            let (source, native) = fixture.run(mode, &["never"], responses);
            assert_eq!(source.1, code);
            compare_outcomes(&fixture, source, native);
        }
    }
}

#[test]
fn print_repeated_signal_exits_while_the_first_shutdown_is_pending() {
    // Frozen print-mode.ts:70-78 registers persistent listeners; disposed (:59)
    // makes the second signal's dispose return, and its finally exits immediately.
    for (first, second, code) in [
        (rustix::process::Signal::TERM, rustix::process::Signal::TERM, 143),
        (rustix::process::Signal::HUP, rustix::process::Signal::HUP, 129),
        (rustix::process::Signal::TERM, rustix::process::Signal::HUP, 129),
        (rustix::process::Signal::HUP, rustix::process::Signal::TERM, 143),
    ] {
        let fixture = Fixture::new();
        let (source, native) = fixture.run_repeated_signal(first, second);
        assert_eq!(source.1, code);
        compare_outcomes(&fixture, source, native);
    }
}

#[test]
fn print_json_preserves_large_output_content_and_event_order() {
    let fixture = Fixture::new();
    let text = "large-λ-🦀\n".repeat(2500);
    let (source, native) = fixture.run(
        "print-json",
        &["large"],
        json!([{"text":text,"endTurn":"large output finished","toolId":"large-call"}]),
    );
    assert!(source.0.iter().any(|line| line.contains("large output finished")));
    assert!(source.0.iter().map(String::len).sum::<usize>() > 100_000);
    compare_outcomes(&fixture, source, native);
}

#[test]
fn print_reload_action_awaits_runtime_and_returns_original_rejection() {
    run_reload_probe(false, false);
}

#[test]
fn print_reload_action_serializes_with_runtime_reload() {
    run_reload_probe(true, false);
}

#[test]
fn print_reload_success_retains_the_old_command_context() {
    run_reload_probe(false, true);
}

fn run_reload_probe(fifo: bool, success: bool) {
    for mode in ["print-text", "print-json"] {
        let fixture = Fixture::new();
        let responses = pi_js_compat::js_json!([{"printFixture":{"reloadProbe":{
            "message":JsString::from("reload-")+JsString::from_utf16(vec![0xd800]),
            "fifo":fifo,"success":success
        }}}]);
        let (source, native) = fixture.run_reload_gate(mode, responses);
        let outcome = |root: &str| {
            pi_js_compat::js_json::parse_lossless(
                std::fs::read_to_string(Path::new(root).join("print-reload-outcome.json")).unwrap(),
            )
            .unwrap()
        };
        let expected = if success {
            pi_js_compat::js_json!({"returned":true,"oldContextIdle":true})
        } else {
            pi_js_compat::js_json!({"rejected":{"kind":"error","value":{"name":"Error","message":JsString::from("reload-")+JsString::from_utf16(vec![0xd800])},"typeError":false}})
        };
        assert_eq!(outcome(&fixture.ts_state), expected);
        compare_outcomes(&fixture, source, native);
        assert_eq!(outcome(&fixture.rust_state), expected);
    }
}

#[test]
fn print_catch_uses_js_string_and_finally_preserves_override_order() {
    let mut outcomes = Vec::new();
    for mode in ["print-text", "print-json"] {
        for config in [
            pi_js_compat::js_json!({"capture":true,"traceDispose":true,"promptRejection":{"kind":"raw","value":JsString::from("prompt-")+JsString::from_utf16(vec![0xd800])}}),
            pi_js_compat::js_json!({"capture":true,"traceDispose":true,"promptRejection":{"kind":"raw","value":{"toString":0}}}),
            pi_js_compat::js_json!({"capture":true,"traceDispose":true,"promptRejection":{"kind":"raw","value":{"toString":0}},"disposeRejection":{"kind":"error","name":"TypeError","message":"dispose overrides catch coercion","code":"EOVERRIDE"}}),
        ] {
            let fixture = Fixture::new();
            let (source, native) = fixture.run(mode, &["trigger"], pi_js_compat::js_json!([{"printFixture":config}]));
            eprintln!("catch mode={mode}: frozen={}, native={}", source.1, native.1);
            outcomes.push((fixture, source, native));
        }
    }
    for (fixture, source, native) in outcomes {
        compare_outcomes(&fixture, source, native);
        let report = |root: &str| {
            pi_js_compat::js_json::parse_lossless(
                std::fs::read_to_string(Path::new(root).join("print-report.json")).unwrap(),
            )
            .unwrap()
        };
        assert_eq!(report(&fixture.rust_state), report(&fixture.ts_state));
        let trace = |root: &str| std::fs::read_to_string(Path::new(root).join("print-trace.jsonl")).unwrap();
        assert_eq!(trace(&fixture.rust_state), trace(&fixture.ts_state));
    }
}

#[test]
fn print_signals_exit_with_frozen_code_even_when_dispose_rejects() {
    let mut outcomes = Vec::new();
    for (signal, code) in [
        (rustix::process::Signal::TERM, 143),
        (rustix::process::Signal::HUP, 129),
    ] {
        let fixture = Fixture::new();
        let responses = pi_js_compat::js_json!([{"pending":true,"printFixture":{
            "disposeRejection":{"kind":"error","name":"TypeError","message":"signal dispose failed","code":"EPRINT"}
        }}]);
        let (source, native) = fixture.run_signalled("print-text", responses, signal);
        eprintln!("signal={signal:?}: frozen={}, native={}", source.1, native.1);
        outcomes.push((fixture, code, source, native));
    }
    for (fixture, code, source, native) in outcomes {
        assert_eq!(source.1, code, "actual frozen signal exit");
        compare_outcomes(&fixture, source, native);
    }
}

#[test]
fn print_mode_finally_rejects_with_original_dispose_error() {
    for mode in ["print-text", "print-json"] {
        for rejection in [
            pi_js_compat::js_json!({"kind":"error","name":"TypeError","message":JsString::from("dispose-")+JsString::from_utf16(vec![0xd800]),"code":"EPRINT"}),
            pi_js_compat::js_json!({"kind":"raw","value":JsString::from("raw-dispose-")+JsString::from_utf16(vec![0xdc00])}),
        ] {
            let fixture = Fixture::new();
            compare(
                &fixture,
                mode,
                &[],
                pi_js_compat::js_json!([{"printFixture":{"capture":true,"disposeRejection":rejection}}]),
            );
            let report = |root: &str| {
                pi_js_compat::js_json::parse_lossless(
                    std::fs::read_to_string(Path::new(root).join("print-report.json")).unwrap(),
                )
                .unwrap()
            };
            assert_eq!(report(&fixture.rust_state), report(&fixture.ts_state));
            let trace = |root: &str| std::fs::read_to_string(Path::new(root).join("print-trace.jsonl")).unwrap();
            assert_eq!(trace(&fixture.rust_state), trace(&fixture.ts_state));
        }
    }
}

#[test]
fn print_text_skips_empty_initial_message_even_with_images() {
    let fixture = Fixture::new();
    compare(
        &fixture,
        "print-text",
        &[""],
        json!([{
            "text":"image-only request should not run", "endTurn":"image-done", "toolId":"image-call",
            "initialImages":[{"type":"image","mimeType":"image/png","data":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg=="}]
        }]),
    );
}

#[test]
fn print_text_omits_end_turn_only_messages() {
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
