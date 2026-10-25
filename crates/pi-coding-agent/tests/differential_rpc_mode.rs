//! Differential coverage for `modes/rpc/rpc-mode.ts`.
//!
//! Drives the real pinned TypeScript RPC mode and the Rust port over stdin/stdout
//! with the faux provider and compares every response/event line byte-for-byte
//! after substituting ids, timestamps, durations, and temp paths.

use crate::mode_harness;

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
                ("PI_CODING_AGENT_STATE_DIR", state.clone()),
                ("PI_HARNESS_PRINT_ROOT", state.clone()),
                ("PI_HARNESS_RPC_ROOT", state),
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
            "captured contexts source={} native={}\nsource events={}\nnative events={}",
            std::fs::read_to_string(&self.context_logs[0])
                .unwrap_or_else(|error| format!("capture unavailable: {error}")),
            std::fs::read_to_string(&self.context_logs[1])
                .unwrap_or_else(|error| format!("capture unavailable: {error}")),
            self.ts.collected.join("\n"),
            self.rust.collected.join("\n")
        );
    }
}

#[test]
fn rpc_prompt_images_preserve_absent_empty_and_populated_extension_input() {
    let (mut session, root) = Session::start_with(&[("PI_HARNESS_RPC_FIXTURE", r#"{"imagesProbe":true}"#)]);
    let image = json!({"type":"image","data":"AA==","mimeType":"image/png"});
    for command in [
        json!({"id":"absent","type":"prompt","message":"absent"}),
        json!({"id":"empty","type":"prompt","message":"empty","images":[]}),
        json!({"id":"image","type":"prompt","message":"image","images":[image.clone()]}),
    ] {
        let (source, native) = session.command(command);
        session.assert_same(&source, &native);
    }
    let read =
        |runtime: &str| std::fs::read_to_string(root.path().join(runtime).join("state/images-report.jsonl")).unwrap();
    let source = read("ts");
    let native = read("rust");
    assert_eq!(source, native);
    let values: Vec<Value> = source.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(values, vec![json!("absent"), json!([]), json!([image])]);
    let normalizer = session.normalizer;
    let (source, source_code, source_error) = session.ts.finish();
    let (native, native_code, native_error) = session.rust.finish();
    assert_eq!((source_code, native_code), (0, 0));
    assert_eq!(normalizer.normalize(&source_error), normalizer.normalize(&native_error));
    let normalize = |lines: Vec<String>| lines.iter().map(|line| normalizer.normalize(line)).collect::<Vec<_>>();
    assert_eq!(normalize(source), normalize(native));
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
fn rpc_callable_status_evaluates_undefined_and_preserves_original_throw() {
    for config in [r#"{"status":"value"}"#, r#"{"status":"omit"}"#, r#"{"status":"throw"}"#] {
        let (mut session, root) = Session::start_with(&[("PI_HARNESS_RPC_FIXTURE", config)]);
        let (source, native) = session.command(json!({"id":"ready","type":"get_state"}));
        session.assert_same(&source, &native);
        let read = |runtime: &str, name: &str| {
            std::fs::read_to_string(root.path().join(runtime).join("state").join(name)).unwrap()
        };
        assert_eq!(read("ts", "status-calls.jsonl"), "null\n");
        assert_eq!(read("rust", "status-calls.jsonl"), "null\n");
        assert_eq!(
            read("ts", "status-report.json"),
            read("rust", "status-report.json"),
            "original callback return/rejection"
        );
        let normalizer = session.normalizer;
        let (source, source_code, source_error) = session.ts.finish();
        let (native, native_code, native_error) = session.rust.finish();
        assert_eq!(source_code, native_code);
        assert_eq!(normalizer.normalize(&source_error), normalizer.normalize(&native_error));
        assert_eq!(source.len(), native.len());
        for (source, native) in source.iter().zip(&native) {
            assert_eq!(normalizer.normalize(source), normalizer.normalize(native));
        }
    }
}

#[test]
fn rpc_audit_rebinding_preserves_replacement_observers() {
    let (mut session, root) = Session::start_with(&[("PI_HARNESS_PRINT_FIXTURE", r#"{"rebindProbe":true}"#)]);
    let (source, native) = session.command(json!({"id":"new","type":"new_session"}));
    session.assert_same(&source, &native);
    session.send_both(r#"{"id":"prompt","type":"prompt","message":"hello"}"#);
    let (source, native) = session.response("prompt");
    session.assert_same(&source, &native);
    session.ts.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    session.rust.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
    for name in ["session", "agent"] {
        let path = |runtime: &str| {
            root.path()
                .join(runtime)
                .join("state")
                .join(format!("print-observer-{name}.json"))
        };
        assert_eq!(std::fs::read_to_string(path("ts")).unwrap(), "{\"observed\":true}");
        assert_eq!(std::fs::read_to_string(path("rust")).unwrap(), "{\"observed\":true}");
    }
}

#[test]
fn rpc_audit_bash_does_not_emit_the_old_only_user_bash_hook() {
    let (mut session, root) = Session::start_with(&[("PI_HARNESS_RPC_FIXTURE", r#"{"bashHook":true}"#)]);
    let (source, native) = session.command(json!({"id":"bash","type":"bash","command":"printf hello"}));
    session.assert_same(&source, &native);
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
    for runtime in ["ts", "rust"] {
        assert!(
            !root.path().join(runtime).join("state/bash-hook.jsonl").exists(),
            "{runtime} emitted user_bash"
        );
    }
}

#[test]
fn rpc_audit_takes_over_stdout_before_binding_extensions() {
    let (mut session, root) = Session::start_with(&[("PI_HARNESS_RPC_FIXTURE", r#"{"stdoutProbe":true}"#)]);
    let (source, native) = session.command(json!({"id":"ready","type":"get_state"}));
    session.assert_same(&source, &native);
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
    for runtime in ["ts", "rust"] {
        assert_eq!(
            std::fs::read_to_string(root.path().join(runtime).join("state/stdout-probe.json")).unwrap(),
            "{\"takenOver\":true}"
        );
    }
}

#[test]
fn rpc_unknown_command_retains_raw_type_and_error_text() {
    let (mut session, _root) = Session::start();
    session.send_both(r#"{"id":"unknown-raw","type":"odd\ud800"}"#);
    let (source, native) = session.response("unknown-raw");
    let decoded = pi_js_compat::js_json::parse_lossless(&source).unwrap();
    let label =
        pi_js_compat::js_json::JsString::from("odd") + pi_js_compat::js_json::JsString::from_utf16(vec![0xd800]);
    assert_eq!(
        decoded["command"],
        pi_js_compat::js_json::JsValue::String(label.clone())
    );
    assert_eq!(
        decoded["error"],
        pi_js_compat::js_json::JsValue::String(pi_js_compat::js_json::JsString::from("Unknown command: ") + label)
    );
    session.assert_same(&source, &native);
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
}

#[test]
fn rpc_external_unknown_command_echoes_original_json_values() {
    let (mut session, _root) = Session::start();
    let mut pairs = Vec::new();
    for (index, (tag, label)) in [
        (json!(false), "false"),
        (json!(17), "17"),
        (Value::Null, "null"),
        (json!([1, 2]), "1,2"),
        (json!({"label":"x"}), "[object Object]"),
        (json!({"toString":0}), "Cannot convert object to primitive value"),
    ]
    .into_iter()
    .enumerate()
    {
        let (source, native) = session.command(json!({"id":format!("unknown-{index}"),"type":tag}));
        let parsed: Value = serde_json::from_str(&source).unwrap();
        assert_eq!(parsed["command"], tag);
        let expected_error = if index == 5 {
            label.to_owned()
        } else {
            format!("Unknown command: {label}")
        };
        assert_eq!(parsed["error"], expected_error);
        pairs.push((source, native));
    }
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
    for (source, native) in pairs {
        assert_eq!(
            session.normalizer.normalize(&source),
            session.normalizer.normalize(&native)
        );
    }
}

#[test]
fn rpc_external_model_lookup_uses_strict_raw_value_equality() {
    let (mut session, _root) = Session::start();
    let (source, native) =
        session.command(json!({"id":"model-raw","type":"set_model","provider":false,"modelId":"faux-1"}));
    let decoded: Value = serde_json::from_str(&source).unwrap();
    assert_eq!(decoded["error"], "Model not found: false/faux-1");
    session.assert_same(&source, &native);
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
}

#[test]
fn rpc_external_core_boundaries_reject_instead_of_coercing_or_dropping() {
    let (mut session, _root) = Session::start();
    let cases = [
        (json!({"type":"prompt","message":17}), "prompt.message"),
        (json!({"type":"prompt","message":"x","images":null}), "prompt.images"),
        (
            json!({"type":"prompt","message":"x","streamingBehavior":false}),
            "prompt.streamingBehavior",
        ),
        (json!({"type":"steer","message":17}), "steer.message"),
        (json!({"type":"steer","message":"x","images":17}), "steer.images"),
        (json!({"type":"follow_up","message":17}), "follow_up.message"),
        (
            json!({"type":"follow_up","message":"x","images":17}),
            "follow_up.images",
        ),
        (
            json!({"type":"new_session","parentSession":17}),
            "new_session.parentSession",
        ),
        (
            json!({"type":"set_thinking_level","level":17}),
            "set_thinking_level.level",
        ),
        (
            json!({"type":"set_steering_mode","mode":false}),
            "set_steering_mode.mode",
        ),
        (
            json!({"type":"set_follow_up_mode","mode":false}),
            "set_follow_up_mode.mode",
        ),
        (
            json!({"type":"compact","customInstructions":false}),
            "compact.customInstructions",
        ),
        (
            json!({"type":"set_auto_compaction","enabled":17}),
            "set_auto_compaction.enabled",
        ),
        (json!({"type":"set_auto_retry","enabled":17}), "set_auto_retry.enabled"),
        (json!({"type":"bash","command":false}), "bash.command"),
        (
            json!({"type":"bash","command":"printf x","excludeFromContext":17}),
            "bash.excludeFromContext",
        ),
        (json!({"type":"export_html","outputPath":17}), "export_html.outputPath"),
        (
            json!({"type":"switch_session","sessionPath":17}),
            "switch_session.sessionPath",
        ),
        (json!({"type":"fork","entryId":false}), "fork.entryId"),
    ];
    for (index, (mut command, field)) in cases.into_iter().enumerate() {
        let id = format!("boundary-{index}");
        command["id"] = json!(id);
        session.rust.send(&command.to_string());
        let response: Value = serde_json::from_str(&session.rust.wait_for_id(&id)).unwrap();
        assert_eq!(response["success"], false);
        let error = response["error"].as_str().unwrap();
        assert!(
            error.contains(field) && error.contains("typed core boundary"),
            "{error}"
        );
    }
    let (source, native) = session.command(json!({"id":"image-extra","type":"prompt","message":"x","images":[{"type":"image","data":"YQ==","mimeType":"image/png","extra":1}]}));
    session.assert_same(&source, &native);
    assert_eq!(serde_json::from_str::<Value>(&native).unwrap()["success"], true);
    session.ts.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    session.rust.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    let (source, native) = session.command(json!({"id":"image-extra-messages","type":"get_messages"}));
    session.assert_same(&source, &native);
    let response: Value = serde_json::from_str(&native).unwrap();
    let image = response["data"]["messages"][0]["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["type"] == "image")
        .unwrap();
    assert_eq!(
        image,
        &json!({"type":"image","data":"YQ==","mimeType":"image/png","extra":1})
    );
    for (index, field) in ["data", "mimeType"].into_iter().enumerate() {
        let id = format!("image-scalar-{index}");
        let image = if field == "data" {
            r#"{"type":"image","data":"\ud800","mimeType":"image/png"}"#
        } else {
            r#"{"type":"image","data":"YQ==","mimeType":"image/\ud800"}"#
        };
        session.rust.send(&format!(
            r#"{{"id":"{id}","type":"prompt","message":"x","images":[{image}]}}"#
        ));
        let response: Value = serde_json::from_str(&session.rust.wait_for_id(&id)).unwrap();
        assert!(
            response["error"]
                .as_str()
                .unwrap()
                .contains(&format!("prompt.images[0].{field}"))
        );
    }
    let (source, native) = session.command(json!({"id":"still-alive","type":"get_state"}));
    session.assert_same(&source, &native);
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
}

#[test]
fn rpc_external_local_trim_and_since_use_frozen_js_operations() {
    let (mut session, _root) = Session::start();
    for (index, name) in [Value::Null, json!(17), json!({}), json!(" \u{a0} raw name \u{feff} ")]
        .into_iter()
        .enumerate()
    {
        let (source, native) =
            session.command(json!({"id":format!("trim-{index}"),"type":"set_session_name","name":name}));
        session.assert_same(&source, &native);
    }
    for (index, since) in [Value::Null, json!(false), json!([1, 2]), json!({"toString":0})]
        .into_iter()
        .enumerate()
    {
        let (source, native) =
            session.command(json!({"id":format!("since-{index}"),"type":"get_entries","since":since}));
        session.assert_same(&source, &native);
    }
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
}

#[test]
fn rpc_external_falsey_follow_up_images_use_the_first_core_truthiness_test() {
    let (mut session, _root) = Session::start();
    for (index, images) in [Value::Null, json!(false), json!(0), json!("")].into_iter().enumerate() {
        let (source, native) = session
            .command(json!({"id":format!("images-{index}"),"type":"follow_up","message":"queued","images":images}));
        session.assert_same(&source, &native);
    }
    let (source, native) = session.command(json!({"id":"queued-state","type":"get_state"}));
    session.assert_same(&source, &native);
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
}

#[test]
fn rpc_external_falsey_steer_images_and_export_path_follow_core_truthiness() {
    let (mut session, _root) = Session::start();
    let (source, native) =
        session.command(json!({"id":"steer-images","type":"steer","message":"hello","images":false}));
    session.assert_same(&source, &native);
    // Frozen session-manager.ts:1698 persists a new session only after an assistant entry.
    let (source, native) = session.command(json!({"id":"export-entry","type":"prompt","message":"seed export"}));
    session.assert_same(&source, &native);
    session.ts.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    session.rust.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    for (index, output) in [Value::Null, json!(false), json!(0), json!("")].into_iter().enumerate() {
        let (source, native) =
            session.command(json!({"id":format!("export-{index}"),"type":"export_html","outputPath":output}));
        assert_eq!(serde_json::from_str::<Value>(&source).unwrap()["success"], true);
        session.assert_same(&source, &native);
    }
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
}

fn assert_rpc_ui_roundtrip(replies: &[(&str, &str)], expected: &str) {
    let (mut session, root) = Session::start_with(&[("PI_HARNESS_RPC_FIXTURE", r#"{"uiProbe":true}"#)]);
    session.send_both(r#"{"id":"dialogs","type":"prompt","message":"/rpc-ui-fixture"}"#);
    for &(method, fields) in replies {
        let source = session
            .ts
            .wait_for(|line| line.contains("\"type\":\"extension_ui_request\""));
        let native = session
            .rust
            .wait_for(|line| line.contains("\"type\":\"extension_ui_request\""));
        session.assert_same(&source, &native);
        let source: Value = serde_json::from_str(&source).unwrap();
        let native: Value = serde_json::from_str(&native).unwrap();
        assert_eq!(source["method"], method);
        for (harness, request) in [(&mut session.ts, source), (&mut session.rust, native)] {
            harness.send(&format!(
                r#"{{"type":"extension_ui_response","id":{}, {fields}}}"#,
                request["id"]
            ));
        }
    }
    let (source, native) = session.response("dialogs");
    session.assert_same(&source, &native);
    let report =
        |runtime: &str| std::fs::read_to_string(root.path().join(runtime).join("state/ui-report.json")).unwrap();
    assert_eq!(report("ts"), expected);
    assert_eq!(report("rust"), expected);
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
}

#[test]
fn rpc_ui_responses_apply_cancellation_before_value_and_preserve_raw_text() {
    assert_rpc_ui_roundtrip(
        &[
            ("select", r#""cancelled":true,"value":"must-not-return""#),
            ("confirm", r#""cancelled":true,"confirmed":true"#),
            ("input", r#""value":"input-\udc00""#),
            ("editor", r#""cancelled":false,"value":"editor-\ud800""#),
        ],
        r#"{"values":[null,false,"input-\udc00","editor-\ud800"]}"#,
    );
}

#[test]
fn rpc_ui_empty_replies_resolve_missing_field_defaults() {
    assert_rpc_ui_roundtrip(
        &[
            ("select", r#""ignored":true"#),
            ("confirm", r#""ignored":true"#),
            ("input", r#""ignored":true"#),
            ("editor", r#""ignored":true"#),
        ],
        r#"{"values":[null,false,null,null]}"#,
    );
}

#[test]
fn rpc_ui_overflow_timeout_uses_node_delay_normalization() {
    let (mut session, root) =
        Session::start_with(&[("PI_HARNESS_RPC_FIXTURE", r#"{"uiProbe":true,"uiTimeout":2147483648}"#)]);
    session.send_both(r#"{"id":"timeout","type":"prompt","message":"/rpc-ui-fixture"}"#);
    for harness in [&mut session.ts, &mut session.rust] {
        let request: Value =
            serde_json::from_str(&harness.wait_for(|line| line.contains("\"type\":\"extension_ui_request\""))).unwrap();
        assert_eq!(request["method"], "select");
        assert_eq!(request["timeout"], 2147483648u64);
    }
    for runtime in ["ts", "rust"] {
        let path = root.path().join(runtime).join("state/ui-selected.json");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !path.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            path.exists(),
            "{runtime}: overflow timer must resolve using Node's 1ms clamp"
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), "{\"value\":null}");
    }
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert!(source_error.contains("TimeoutOverflowWarning"), "{source_error}");
    assert!(native_error.is_empty(), "{native_error}");
}

#[test]
fn rpc_external_response_ids_retain_every_json_value() {
    let (mut session, _root) = Session::start();
    for id in [
        Value::Null,
        json!(false),
        json!(17),
        json!([1, 2]),
        json!({"value":"id"}),
    ] {
        session.send_both(&json!({"id":id,"type":"get_tree"}).to_string());
        let predicate =
            |line: &str| line.contains("\"type\":\"response\"") && line.contains("\"command\":\"get_tree\"");
        let source = session.ts.wait_for(predicate);
        let native = session.rust.wait_for(predicate);
        assert_eq!(serde_json::from_str::<Value>(&source).unwrap()["id"], id);
        session.assert_same(&source, &native);
    }
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
}

#[test]
fn rpc_fire_and_forget_ui_methods_emit_exact_requests_without_editor_state() {
    let (mut session, root) = Session::start_with(&[("PI_HARNESS_RPC_FIXTURE", r#"{"fireProbe":true}"#)]);
    session.send_both(r#"{"id":"fire","type":"prompt","message":"/rpc-fire-fixture"}"#);
    let (source, native) = session.response("fire");
    session.assert_same(&source, &native);
    let normalizer = session.normalizer;
    let (source, source_code, source_error) = session.ts.finish();
    let (native, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
    assert_eq!(source.len(), 10);
    assert_eq!(source.len(), native.len());
    for (source, native) in source.iter().zip(&native) {
        assert_eq!(normalizer.normalize(source), normalizer.normalize(native));
    }
    for runtime in ["ts", "rust"] {
        assert_eq!(
            std::fs::read_to_string(root.path().join(runtime).join("state/fire-report.json")).unwrap(),
            "{\"editor\":\"\"}"
        );
    }
}

#[test]
fn rpc_extension_shutdown_request_is_checked_after_the_next_command() {
    let (mut session, _root) = Session::start_with(&[("PI_HARNESS_RPC_FIXTURE", r#"{"shutdownCommand":true}"#)]);
    session.send_both(r#"{"id":"shutdown","type":"prompt","message":"/rpc-shutdown-fixture"}"#);
    let (source, native) = session.response("shutdown");
    session.assert_same(&source, &native);
    let (source, native) = session.command(json!({"id":"exit-state","type":"get_state"}));
    session.assert_same(&source, &native);
    let normalizer = session.normalizer;
    let (source, source_code, source_error) = session.ts.finish_with_exit_deadline(std::time::Duration::from_secs(10));
    let (native, native_code, native_error) = session
        .rust
        .finish_with_exit_deadline(std::time::Duration::from_secs(10));
    assert_eq!(source_code, 0);
    assert_eq!(source_code, native_code);
    assert_eq!(source.len(), native.len());
    for (source, native) in source.iter().zip(&native) {
        assert_eq!(normalizer.normalize(source), normalizer.normalize(native));
    }
    assert_eq!(source_error, native_error);
}

#[test]
fn rpc_switch_session_restores_the_original_session_and_rebinds_events() {
    let (mut session, _root) = Session::start();
    let (source, native) = session.command(json!({"id":"saved-prompt","type":"prompt","message":"hello"}));
    session.assert_same(&source, &native);
    session.ts.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    session.rust.wait_for(|line| line.contains("\"type\":\"agent_end\""));
    let (source_state, native_state) = session.command(json!({"id":"saved-state","type":"get_state"}));
    session.assert_same(&source_state, &native_state);
    let source_state: Value = serde_json::from_str(&source_state).unwrap();
    let native_state: Value = serde_json::from_str(&native_state).unwrap();
    let (source, native) = session.command(json!({"id":"fresh","type":"new_session"}));
    session.assert_same(&source, &native);
    session.ts.send(
        &json!({"id":"restored","type":"switch_session","sessionPath":source_state["data"]["sessionFile"]}).to_string(),
    );
    session.rust.send(
        &json!({"id":"restored","type":"switch_session","sessionPath":native_state["data"]["sessionFile"]}).to_string(),
    );
    let (source, native) = session.response("restored");
    session.assert_same(&source, &native);
    assert_eq!(
        serde_json::from_str::<Value>(&source).unwrap()["data"]["cancelled"],
        false
    );
    let (source, native) = session.command(json!({"id":"restored-state","type":"get_state"}));
    session.assert_same(&source, &native);
    let restored: Value = serde_json::from_str(&source).unwrap();
    assert_eq!(restored["data"]["sessionId"], source_state["data"]["sessionId"]);
    assert_eq!(restored["data"]["messageCount"], source_state["data"]["messageCount"]);
    let (_, source_code, source_error) = session.ts.finish();
    let (_, native_code, native_error) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(source_error, native_error);
}

#[test]
fn rpc_sighup_detach_does_not_synthesize_eof_while_stdout_flush_is_blocked() {
    // Frozen :731-738 removes onInputEnd before flushing. Aborting a native reader
    // must not invent another shutdown(0) while the signal shutdown awaits the pipe.
    let (mut session, root) = Session::start_with(&[
        (
            "PI_HARNESS_RPC_FIXTURE",
            r#"{"earlySignal":"SIGHUP","shutdownFlood":true}"#,
        ),
        ("PI_HARNESS_PAUSE_STDOUT", "1"),
    ]);
    session.send_both(r#"{"id":"flood","type":"prompt","message":"/rpc-flood-signal"}"#);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    for runtime in ["ts", "rust"] {
        let marker = root.path().join(runtime).join("state/rpc-shutdown.json");
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "{runtime} shutdown must reach disposal; source={} native={}",
                session.ts.stderr(),
                session.rust.stderr()
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    let source_waiting = session.ts.is_running();
    let native_waiting = session.rust.is_running();
    for runtime in ["ts", "rust"] {
        std::fs::write(root.path().join(runtime).join("state/rpc-stdout-release"), "drain").unwrap();
    }
    let (source, source_code, source_error) = session.ts.finish_with_exit_deadline(std::time::Duration::from_secs(20));
    let (native, native_code, native_error) = session
        .rust
        .finish_with_exit_deadline(std::time::Duration::from_secs(20));
    assert!(source_waiting, "source must await blocked stdout");
    assert!(
        native_waiting,
        "native must await blocked stdout instead of synthetic EOF"
    );
    assert_eq!(source_code, 129, "source stderr: {source_error}");
    assert_eq!(native_code, 129, "native stderr: {native_error}");
    assert_eq!(source_error, native_error);
    let source: Value = source
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|value| value["method"] == "notify")
        .expect("source notification");
    let native: Value = native
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|value| value["method"] == "notify")
        .expect("native notification");
    assert_eq!(source["message"], "x".repeat(512 * 1024));
    assert_eq!(native["message"], source["message"]);
}

fn assert_rpc_startup_signal(config: &'static str, code: i32) {
    let (session, root) = Session::start_with(&[("PI_HARNESS_RPC_FIXTURE", config)]);
    let (source, source_code, source_error) = session.ts.finish_with_exit_deadline(std::time::Duration::from_secs(20));
    let (native, native_code, native_error) = session
        .rust
        .finish_with_exit_deadline(std::time::Duration::from_secs(20));
    assert_eq!(source_code, code, "source stderr: {source_error}");
    assert_eq!(native_code, code, "native stderr: {native_error}");
    assert_eq!(source, native);
    assert_eq!(source_error, native_error);
    for runtime in ["ts", "rust"] {
        assert_eq!(
            std::fs::read_to_string(root.path().join(runtime).join("state/rpc-shutdown.json")).unwrap(),
            "{\"disposed\":true}"
        );
    }
}

#[test]
fn rpc_signal_sigterm_at_first_startup_turn_disposes_and_exits_143() {
    assert_rpc_startup_signal(r#"{"earlySignal":"SIGTERM"}"#, 143);
}

#[test]
fn rpc_signal_sighup_at_first_startup_turn_disposes_and_exits_129() {
    assert_rpc_startup_signal(r#"{"earlySignal":"SIGHUP"}"#, 129);
}

fn assert_rpc_pending_disposal_signal(signal: rustix::process::Signal, expected_code: i32) {
    let (session, root) = Session::start_with(&[(
        "PI_HARNESS_RPC_FIXTURE",
        r#"{"earlySignal":"SIGTERM","pendingShutdown":true}"#,
    )]);
    let signal_pending = |harness: &ModeHarness, runtime: &str| {
        let marker = root.path().join(runtime).join("state/rpc-shutdown-ready.json");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "{runtime} must enter pending disposal"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let ready: Value = serde_json::from_slice(&std::fs::read(marker).unwrap()).unwrap();
        harness.signal_actual_process(u32::try_from(ready["pid"].as_u64().unwrap()).unwrap(), signal);
    };
    signal_pending(&session.ts, "ts");
    signal_pending(&session.rust, "rust");
    let (source, source_code, source_error) = session.ts.finish_with_exit_deadline(std::time::Duration::from_secs(5));
    let (native, native_code, native_error) = session
        .rust
        .finish_with_exit_deadline(std::time::Duration::from_secs(5));
    assert_eq!(source_code, expected_code, "frozen default signal handler exit");
    assert_eq!(native_code, source_code);
    assert_eq!(source, native);
    assert_eq!(source_error, native_error);
}

#[test]
fn rpc_signal_other_listener_is_removed_before_pending_disposal() {
    assert_rpc_pending_disposal_signal(rustix::process::Signal::HUP, 129);
}

#[test]
fn rpc_signal_same_signal_exits_while_first_disposal_is_pending() {
    assert_rpc_pending_disposal_signal(rustix::process::Signal::TERM, 143);
}

#[test]
fn rpc_bash_response_preserves_raw_request_id_without_chunk_events() {
    let (mut session, _root) = Session::start();
    session.send_both(r#"{"id":"\ud800","type":"bash","command":"printf x"}"#);
    let is_response = |line: &str| line.contains("\"type\":\"response\"") && line.contains("\"command\":\"bash\"");
    let source = session.ts.wait_for(is_response);
    let native = session.rust.wait_for(is_response);
    session.assert_same(&source, &native);
    let decoded = pi_js_compat::js_json::parse_lossless(&native).unwrap();
    assert_eq!(decoded["id"].as_string().unwrap().units(), &[0xd800]);
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
        let is_clone = command["type"] == "clone";
        let (ts, rust) = session.command(command);
        if is_clone {
            // The unsaved fork holds the context entry; each run generated its own ID.
            // Assert the entire serialized response, including the exact referenced subject.
            for (response, entry_id) in [(&ts, &ts_entry_id), (&rust, &rust_entry_id)] {
                let expected = pi_js_compat::js_json!({"id":"cl1","type":"response","command":"clone","success":false,"error":format!("Entry {entry_id} not found")});
                assert_eq!(response, &pi_js_compat::js_json::stringify_lossless(&expected));
            }
        } else {
            session.assert_same(&ts, &rust);
        }
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

// AGENTS.md approved2026-10-23: async faux-factory abort microtasks need not reproduce
// the input/cacheRead allocation. This comparator is used only by this one RPC case.
fn assert_async_factory_timeout_line(normalizer: &Normalizer, source: &str, native: &str) {
    // Existing duration normalization uses a non-JSON marker; quote that marker for parsing.
    let normalized = |line: &str| normalizer.normalize(line).replace(":<duration>", ":\"<duration>\"");
    let source: Value = serde_json::from_str(&normalized(source)).unwrap();
    let native: Value = serde_json::from_str(&normalized(native)).unwrap();
    assert_async_factory_timeout_value(&source, &native, "$");
}

fn assert_async_factory_timeout_value(source: &Value, native: &Value, path: &str) {
    match (source, native) {
        (Value::Object(source), Value::Object(native)) => {
            // rpc-jsonl.md explicitly permits Rust serializer object-key order.
            assert_eq!(source.len(), native.len());
            assert!(source.keys().all(|key| native.contains_key(key)));
            for (key, value) in source {
                if key == "usage" && source.get("role").and_then(Value::as_str) == Some("assistant") {
                    assert_async_factory_usage(value, &native[key]);
                } else {
                    assert_async_factory_timeout_value(value, &native[key], &format!("{path}.{key}"));
                }
            }
        }
        (Value::Array(source), Value::Array(native)) => {
            assert_eq!(source.len(), native.len());
            for (index, (source, native)) in source.iter().zip(native).enumerate() {
                assert_async_factory_timeout_value(source, native, &format!("{path}[{index}]"));
            }
        }
        _ => assert_eq!(source, native, "{path}"),
    }
}

fn assert_async_factory_usage(source: &Value, native: &Value) {
    let source = source.as_object().expect("assistant usage object");
    let native = native.as_object().expect("assistant usage object");
    assert_eq!(source.len(), native.len());
    assert!(source.keys().all(|key| native.contains_key(key)));
    let input_total = |usage: &serde_json::Map<String, Value>| {
        usage["input"].as_f64().unwrap() + usage["cacheRead"].as_f64().unwrap()
    };
    assert_eq!(input_total(source), input_total(native));
    for (key, value) in source {
        if key != "input" && key != "cacheRead" {
            assert_eq!(value, &native[key], "assistant usage.{key}");
        }
    }
}

fn read_aborted_session_line(root: &Path, runtime: &str) -> String {
    let mut records = Vec::new();
    for entry in std::fs::read_dir(root.join(runtime).join("sessions")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "jsonl") {
            continue;
        }
        for line in std::fs::read_to_string(path).unwrap().lines() {
            let value: Value = serde_json::from_str(line).unwrap();
            if value["type"] == "message"
                && value["message"]["role"] == "assistant"
                && value["message"]["stopReason"] == "aborted"
            {
                records.push(line.to_owned());
            }
        }
    }
    assert_eq!(records.len(), 1, "one persisted aborted assistant per runtime");
    records.pop().unwrap()
}

#[test]
fn thinking_timeout_recovery_matches_frozen_rpc() {
    // The pending first factory must time out; the immediate recovery must not.
    // A 100 ms budget races native event I/O under the parallel process matrix.
    let (mut session, root) = Session::start_with_responses(
        &[
            ("PI_HARNESS_THINKING_TIMEOUT_MS", "1000"),
            ("PI_HARNESS_IMMUTABLE_PARTIALS", "1"),
        ],
        r#"[{"pending":true},{"text":"Recovered","endTurn":"done","toolId":"recovered"}]"#,
    );
    for command in [
        json!({"id":"retry-off","type":"set_auto_retry","enabled":false}),
        json!({"id":"compact-off","type":"set_auto_compaction","enabled":false}),
    ] {
        let (source, native) = session.command(command);
        session.assert_same(&source, &native);
    }
    session.send_both(r#"{"id":"prompt","type":"prompt","message":"Original input"}"#);
    let (source, native) = session.response("prompt");
    session.assert_same(&source, &native);
    let retry_start = |line: &str| line.contains("\"type\":\"auto_retry_start\"");
    let source = session.ts.wait_for(retry_start);
    let native = session.rust.wait_for(retry_start);
    session.assert_same(&source, &native);
    assert!(source.contains("thinking phase exceeded 20 minutes"));
    let retry_end = |line: &str| line.contains("\"type\":\"auto_retry_end\"");
    let source = session.ts.wait_for(retry_end);
    let native = session.rust.wait_for(retry_end);
    session.assert_same(&source, &native);
    assert!(source.contains("\"success\":true"));
    let (source, native) = session.command(json!({"id":"messages","type":"get_messages"}));
    assert_async_factory_timeout_line(&session.normalizer, &source, &native);
    let messages: Value = serde_json::from_str(&source).unwrap();
    assert_eq!(
        messages["data"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["role"] == "user")
            .count(),
        1
    );
    let normalizer = session.normalizer.clone();
    let (source, source_code, source_stderr) = session.ts.finish();
    let (native, native_code, native_stderr) = session.rust.finish();
    assert_eq!(source_code, native_code);
    assert_eq!(
        normalizer.normalize(&source_stderr),
        normalizer.normalize(&native_stderr)
    );
    assert_eq!(source.len(), native.len());
    for (source, native) in source.iter().zip(&native) {
        assert_async_factory_timeout_line(&normalizer, source, native);
    }
    // AGENTS.md:40 permits native AssistantMessage key order, not different values/presence.
    let source_record = read_aborted_session_line(root.path(), "ts");
    let native_record = read_aborted_session_line(root.path(), "rust");
    let source_value: Value = serde_json::from_str(&normalizer.normalize(&source_record)).unwrap();
    let native_value: Value = serde_json::from_str(&normalizer.normalize(&native_record)).unwrap();
    assert_eq!(source_value, native_value);

    // The actual TS-written record still has byte-exact native raw persistence semantics.
    let parsed = pi_coding_agent::core::session_manager::parse_session_entries(source_record.as_str());
    assert_eq!(parsed.len(), 1);
    let serialized =
        pi_coding_agent::core::session_tool_output::serialize_session_entry_for_persistence(&parsed[0]).unwrap();
    assert_eq!(serialized, source_record);
    let round_trip = root.path().join("ts-aborted-native-roundtrip.jsonl");
    std::fs::write(&round_trip, serialized + "\n").unwrap();
    assert_eq!(std::fs::read_to_string(round_trip).unwrap(), source_record + "\n");
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
