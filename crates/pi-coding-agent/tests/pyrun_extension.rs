//! Behavioral tests for the bundled Pyrun runner, eval tool, and extension.

use crate::shared_executable_script as executable_script;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::bundled_extensions::pyrun::eval_tool::{
    STREAMED_CONSOLE_BYTE_LIMIT, append_capped_console_text, cap_console_history, cap_console_text,
    format_canonical_pyrun_eval_result, format_progress_text, format_result_value,
};
use pi_coding_agent::bundled_extensions::pyrun::index::{
    PyrunExtensionOptions, create_pyrun_pi_dispatcher, create_pyrun_tool_definition, install_pyrun_extension,
};
use pi_coding_agent::bundled_extensions::pyrun::runner::{
    CanonicalPyrunConsoleEntry, CanonicalPyrunEvalParams, CanonicalPyrunEvalResult, PiRequest, PyrunRunnerClient,
    PyrunRunnerOptions, resolve_pyrun_runner_options,
};
use pi_coding_agent::core::event_bus::{EventBus, EventBusController};
use pi_coding_agent::core::extensions::types::{
    ExtensionActions, ExtensionContextActions, ExtensionMode, ExtensionStatus, ExtensionUiContext, NotifyType,
    UserMessageContent,
};
use pi_coding_agent::core::extensions::{ExtensionRunner, create_extension_runtime, load_extension_from_factory};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue as Value};

fn temp_dir(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "pi-pyrun-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_str().expect("utf-8 test path").to_owned()
}

fn write_runner(dir: &str, name: &str, body: &str) -> String {
    let path = std::path::Path::new(dir).join(name);
    executable_script::write_executable_script(&path, &format!("#!/usr/bin/env python3\n{body}"));
    path.display().to_string()
}

fn runner_for(dir: &str) -> PyrunRunnerClient {
    PyrunRunnerClient::new(PyrunRunnerOptions {
        command: Some(format!("{dir}/fake-pyrun").into()),
        args: Some(Vec::new()),
        env: Some(
            [(
                "PATH".into(),
                format!("{dir}:{}", std::env::var("PATH").unwrap_or_default()).into(),
            )]
            .into(),
        ),
        inherit_env: Some(true),
        detached: Some(false),
        inherited_process_group_id: None,
    })
}

fn params(code: &str) -> CanonicalPyrunEvalParams {
    CanonicalPyrunEvalParams {
        code: code.into(),
        pi: None,
        pi_bridge: Some(false),
        session_id: None,
        stream_console: Some(true),
    }
}

#[test]
fn resolves_runner_options_from_environment() {
    let env = vec![
        ("PI_PYRUN_RUNNER_COMMAND".into(), "custom-runner".into()),
        ("PI_PYRUN_RUNNER_ARGS".into(), "[\"--flag\",\"value\"]".into()),
    ];
    let resolved = resolve_pyrun_runner_options(Some(&env)).unwrap();
    assert_eq!(resolved.command, "custom-runner");
    assert_eq!(resolved.args, vec!["--flag", "value"]);

    let env = vec![("PI_PYRUN_RUNNER".into(), "legacy-runner".into())];
    assert_eq!(
        resolve_pyrun_runner_options(Some(&env)).unwrap().command,
        "legacy-runner"
    );

    let env = vec![("PI_PYRUN_RUNNER_ARGS".into(), "{}".into())];
    assert_eq!(
        resolve_pyrun_runner_options(Some(&env)).unwrap_err().to_string(),
        "PI_PYRUN_RUNNER_ARGS must be a JSON string array"
    );

    assert_eq!(resolve_pyrun_runner_options(Some(&[])).unwrap().command, "pyrun-jsonl");
}

#[tokio::test]
async fn evaluates_against_the_jsonl_runner_and_reports_progress() {
    let dir = temp_dir("runner");
    write_runner(
        &dir,
        "fake-pyrun",
        r#"
import json, sys
line = sys.stdin.readline()
params = json.loads(line)
print(json.dumps({"type": "console", "text": "hello\n"}), flush=True)
print(json.dumps({"type": "progress", "message": "working"}), flush=True)
print(json.dumps({"type": "completed", "executed": params["code"], "console": ["one", {"level": "warn", "message": "two"}], "value": {"ok": True}}), flush=True)
"#,
    );
    let runner = runner_for(&dir);
    let updates = Arc::new(Mutex::new(Vec::new()));
    let updates_for_callback = Arc::clone(&updates);
    let result = runner
        .evaluate(
            params("print(1)"),
            Some(Arc::new(move |update| {
                updates_for_callback.lock().unwrap().push(update.raw);
                Ok(())
            })),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(result.result_type, "completed");
    assert_eq!(result.executed.as_ref(), Some(&JsString::from("print(1)")));
    assert_eq!(result.value, Some(json!({"ok": true})));
    let updates = updates.lock().unwrap();
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0]["type"], json!("console"));
    assert_eq!(updates[1]["message"], json!("working"));
}

#[tokio::test]
async fn runner_requests_and_bridge_replies_use_javascript_json() {
    let dir = temp_dir("js-wire");
    write_runner(
        &dir,
        "fake-pyrun",
        r#"
import json, sys
request = sys.stdin.readline().strip()
print(json.dumps({"type":"pi_request", "method":"echo", "params":{}}), flush=True)
reply = sys.stdin.readline().strip()
print(json.dumps({"type":"completed", "value":[request,reply]}), flush=True)
"#,
    );
    let runner = runner_for(&dir);
    let mut input = params("x");
    input.pi = Some(js_json::parse_lossless(r#"{"2":2.0,"1":1.0}"#).unwrap());
    let handler = Arc::new(|_: PiRequest| {
        Box::pin(async { Ok(Some(js_json::parse_lossless(r#"{"2":2.0,"1":1.0}"#).unwrap())) })
            as pi_coding_agent::core::extensions::types::BoxFuture<
                'static,
                Result<Option<Value>, pi_coding_agent::core::extensions::types::HandlerError>,
            >
    });
    let result = runner.evaluate(input, None, None, Some(handler)).await.unwrap();
    let wire = result.value.unwrap();
    assert!(
        wire[0].as_string().unwrap().contains(r#""pi":{"1":1,"2":2}"#),
        "{wire:?}"
    );
    assert_eq!(wire[1], json!(r#"{"result":{"1":1,"2":2}}"#));
}

#[tokio::test]
async fn responds_to_pi_requests_over_the_same_stdin() {
    let dir = temp_dir("pi-request");
    write_runner(
        &dir,
        "fake-pyrun",
        r#"
import json, sys
sys.stdin.readline()
print(json.dumps({"type": "pi_request", "method": "models.scoped", "params": {"x": 1}}), flush=True)
response = json.loads(sys.stdin.readline())
print(json.dumps({"type": "completed", "executed": "x", "value": response}), flush=True)
"#,
    );
    let runner = runner_for(&dir);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_for_handler = Arc::clone(&seen);
    let handler = Arc::new(move |request: PiRequest| {
        seen_for_handler.lock().unwrap().push(request.clone());
        Box::pin(async move { Ok(Some(json!({"models": ["m1"]}))) })
            as pi_coding_agent::core::extensions::types::BoxFuture<
                'static,
                Result<Option<Value>, pi_coding_agent::core::extensions::types::HandlerError>,
            >
    });
    let result = runner.evaluate(params("x"), None, None, Some(handler)).await.unwrap();
    assert_eq!(result.value, Some(json!({"result": {"models": ["m1"]}})));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "models.scoped");
    assert_eq!(seen[0].params, json!({"x": 1}));
}

#[tokio::test]
async fn reports_runner_exit_status_and_stderr() {
    let dir = temp_dir("exit");
    write_runner(
        &dir,
        "fake-pyrun",
        r#"
import sys
sys.stdin.readline()
sys.stderr.write("boom\n")
sys.exit(3)
"#,
    );
    let error = runner_for(&dir)
        .evaluate(params("x"), None, None, None)
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "Pyrun runner exited with exit code 3\nboom");
}

#[tokio::test]
async fn reports_missing_runner_with_node_spawn_text() {
    let runner = PyrunRunnerClient::new(PyrunRunnerOptions {
        command: Some("definitely-missing-pyrun".into()),
        ..Default::default()
    });
    let error = runner.evaluate(params("x"), None, None, None).await.unwrap_err();
    assert_eq!(error.to_string(), "spawn definitely-missing-pyrun ENOENT");
}

#[tokio::test]
async fn aborting_an_evaluation_kills_the_runner_and_rejects() {
    let dir = temp_dir("abort");
    write_runner(
        &dir,
        "fake-pyrun",
        r#"
import sys, time
sys.stdin.readline()
time.sleep(30)
"#,
    );
    let runner = runner_for(&dir);
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let controller_signal_for_task = controller_signal.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        controller_signal_for_task.abort(None);
    });
    let error = runner
        .evaluate(params("x"), None, Some(signal), None)
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "Pyrun evaluation aborted");
}

#[test]
fn caps_and_formats_console_output() {
    assert_eq!(cap_console_text(&JsString::from("hello"), 10), "hello");
    assert_eq!(cap_console_text(&JsString::from("hello"), 3), "llo");
    assert_eq!(cap_console_text(&JsString::from("é"), 1), "");
    assert_eq!(
        append_capped_console_text(&JsString::from("a\nb\n"), &JsString::from("c\n")),
        "a\nb\nc\n"
    );
    assert_eq!(STREAMED_CONSOLE_BYTE_LIMIT, 1_048_576);

    let entries = vec![
        CanonicalPyrunConsoleEntry::Text("one".into()),
        CanonicalPyrunConsoleEntry::Leveled {
            level: "warn".into(),
            message: "two".into(),
        },
    ];
    assert_eq!(cap_console_history(&entries), entries);
}

#[test]
fn formats_result_values_like_the_tool_output() {
    let mut result = CanonicalPyrunEvalResult {
        result_type: "completed".into(),
        ..Default::default()
    };
    assert_eq!(format_result_value(&result), None);
    result.value = Some(json!("text"));
    assert_eq!(format_result_value(&result), Some("text".into()));
    result.value = Some(json!("   "));
    assert_eq!(format_result_value(&result), None);
    result.value = Some(json!({"stdout": "", "stderr": "", "exit_code": 0}));
    assert_eq!(format_result_value(&result), None);
    result.value = Some(json!({"stdout": "", "stderr": "", "exit_code": 2}));
    assert_eq!(format_result_value(&result), Some("exit code 2".into()));
    result.value = Some(js_json::parse_lossless(r#"{"2":2.0,"1":1.0}"#).unwrap());
    assert_eq!(
        format_result_value(&result).as_ref(),
        Some(&JsString::from(r#"{"1":1,"2":2}"#))
    );
    result.value = Some(json!({"a": 1}));
    assert_eq!(format_result_value(&result), Some("{\"a\":1}".into()));

    result.result_type = "needs_approval".into();
    assert_eq!(
        format_result_value(&result),
        Some("needs approval: unknown Pyrun operation".into())
    );
}

#[test]
fn formats_progress_updates_in_priority_order() {
    let update =
        |value: Value| pi_coding_agent::bundled_extensions::pyrun::runner::CanonicalPyrunProgressUpdate { raw: value };
    assert_eq!(
        format_progress_text(&update(json!({"type": "pi_request", "method": "models.scoped"}))),
        "Pi request: models.scoped"
    );
    assert_eq!(
        format_progress_text(&update(json!({"type": "console", "message": "m"}))),
        "m"
    );
    assert_eq!(
        format_progress_text(&update(json!({"type": "console", "text": "t"}))),
        "t"
    );
    assert_eq!(
        format_progress_text(&update(json!({"type": "console", "output": "o"}))),
        "o"
    );
    assert_eq!(
        format_progress_text(&update(json!({"type": "console", "status": "s"}))),
        "s"
    );
    assert_eq!(
        format_progress_text(&update(json!({"type": "console", "value": {"a": 1}}))),
        "{\"a\":1}"
    );
    let numeric = js_json::parse_lossless(r#"{"type":"console","value":{"2":2.0,"1":1.0}}"#).unwrap();
    assert_eq!(format_progress_text(&update(numeric)), r#"{"1":1,"2":2}"#);
    assert_eq!(format_progress_text(&update(json!({"type": "other"}))), "other");
}

#[test]
fn formats_the_canonical_tool_result() {
    let result = CanonicalPyrunEvalResult {
        result_type: "completed".into(),
        executed: Some("print(1)".into()),
        console: Some(vec![CanonicalPyrunConsoleEntry::Text("out".into())]),
        error: None,
        value: Some(json!("done")),
        approval: None,
    };
    let formatted = format_canonical_pyrun_eval_result(
        &pi_coding_agent::bundled_extensions::pyrun::eval_tool::PyrunEvalParams {
            code: "print(1)".into(),
            session_id: None,
        },
        result,
    );
    assert_eq!(
        formatted.content,
        vec![TextOrImageContent::Text(TextContent::new("print(1)\n\nout\ndone"))]
    );
    assert_eq!(formatted.is_error, Some(false));
}

// ---------------------------------------------------------------------------
// Extension boundary
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Records {
    entries: Vec<(JsString, Option<Value>)>,
    user_messages: Vec<UserMessageContent>,
    statuses: Vec<(JsString, Option<JsString>)>,
    notifications: Vec<(JsString, Option<NotifyType>)>,
    calls: Vec<(String, Value)>,
}

struct TestUi {
    records: Arc<Mutex<Records>>,
}

impl ExtensionUiContext for TestUi {
    fn notify(
        &self,
        message: &JsString,
        notify_type: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.records
            .lock()
            .unwrap()
            .notifications
            .push((message.into(), notify_type));
        Ok(())
    }

    fn set_status(&self, key: &JsString, text: Option<ExtensionStatus>) -> Result<(), pi_ai::request::CallbackError> {
        let text = text.map(|text| text.evaluate(None)).transpose()?.flatten();
        self.records.lock().unwrap().statuses.push((key.into(), text));
        Ok(())
    }
}

fn event_bus() -> Arc<dyn EventBus> {
    Arc::new(EventBusController::new())
}

async fn model_registry() -> Arc<ModelRegistry> {
    Arc::new(
        ModelRegistry::in_memory(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
            serde_json::Map::new(),
        ))
        .unwrap(),
    )
}

fn actions() -> ExtensionActions {
    ExtensionActions {
        send_message: Arc::new(|_, _| Ok(())),
        send_user_message: Arc::new(|_, _| Ok(())),
        append_entry: Arc::new(|_, _| Ok(())),
        request_resume_continuation: Arc::new(|| Ok(())),
        set_session_name: Arc::new(|_| Ok(())),
        get_session_name: Arc::new(|| Ok(None)),
        set_label: Arc::new(|_, _| Ok(())),
        get_active_tools: Arc::new(|| Ok(Vec::new())),
        get_all_tools: Arc::new(|| Ok(Vec::new())),
        set_active_tools: Arc::new(|_| Ok(())),
        call_tool: Arc::new(|name, params, _, _| {
            Box::pin(async move {
                Ok(AgentToolResult {
                    content: vec![TextOrImageContent::Text(TextContent::new(name))],
                    details: Some(params),
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        call_command: Arc::new(|name, args| Box::pin(async move { Ok(json!({"name": name, "args": args})) })),
        refresh_tools: Arc::new(|| Ok(())),
        get_commands: Arc::new(|| {
            Ok(vec![pi_coding_agent::core::slash_commands::SlashCommandInfo {
                name: "goal".into(),
                description: Some("Goal".into()),
                source: pi_coding_agent::core::slash_commands::SlashCommandSource::Extension,
                source_info: pi_coding_agent::core::source_info::create_synthetic_source_info(
                    &JsString::from("<test>"),
                    &pi_coding_agent::core::source_info::SyntheticSourceInfoOptions {
                        source: "extension".into(),
                        scope: None,
                        origin: None,
                        base_dir: None,
                    },
                ),
            }])
        }),
        set_model: Arc::new(|_| Box::pin(async { Ok(true) })),
        get_thinking_level: Arc::new(|| Ok(pi_agent_core::harness::types::ThinkingLevel::Off)),
        set_thinking_level: Arc::new(|_| Ok(())),
    }
}

fn context_actions() -> ExtensionContextActions {
    ExtensionContextActions {
        get_model: Arc::new(|| None),
        get_thinking_level: None,
        set_thinking_level: None,
        get_scoped_models: Some(Arc::new(Vec::new)),
        get_footer_data: None,
        is_idle: Arc::new(|| true),
        has_active_retry: Arc::new(|| false),
        is_project_trusted: Arc::new(|| true),
        get_signal: Arc::new(|| None),
        abort: Arc::new(|| {}),
        has_pending_messages: Arc::new(|| false),
        shutdown: Arc::new(|| {}),
        restart: Arc::new(|_| Box::pin(async { Ok(()) })),
        get_control_db_path: None,
        get_context_usage: Arc::new(|| None),
        get_multi_agent_agent_id: None,
        get_multi_agent_parent_session_id: None,
        register_supervisor_review_cancellation: None,
        get_no_supervisor: None,
        get_multi_agent_requires_agent_id: None,
        get_multi_agent_store: None,
        get_tool_detach_registry: None,
        compact: Arc::new(|_| {}),
        get_system_prompt: Arc::new(JsString::default),
        get_system_prompt_options: None,
    }
}

struct Harness {
    runner: ExtensionRunner,
    api: pi_coding_agent::core::extensions::ExtensionApi,
    dispatcher: pi_coding_agent::bundled_extensions::pyrun::eval_tool::PyrunPiRequestDispatcher,
    records: Arc<Mutex<Records>>,
    compacted: Arc<AtomicBool>,
    restarted: Arc<AtomicBool>,
}

async fn harness() -> Harness {
    let cwd = temp_dir("extension");
    let records = Arc::new(Mutex::new(Records::default()));
    let compacted = Arc::new(AtomicBool::new(false));
    let restarted = Arc::new(AtomicBool::new(false));
    let runtime = create_extension_runtime();
    let captured: Arc<Mutex<Option<pi_coding_agent::bundled_extensions::pyrun::eval_tool::PyrunPiRequestDispatcher>>> =
        Arc::new(Mutex::new(None));
    let captured_for_factory = Arc::clone(&captured);
    let captured_api: Arc<Mutex<Option<pi_coding_agent::core::extensions::ExtensionApi>>> = Arc::new(Mutex::new(None));
    let captured_api_for_factory = Arc::clone(&captured_api);
    let factory: pi_coding_agent::core::extensions::ExtensionFactory = Arc::new(move |api| {
        let options = PyrunExtensionOptions::default();
        *captured_for_factory.lock().unwrap() = Some(create_pyrun_pi_dispatcher(api, &options));
        *captured_api_for_factory.lock().unwrap() = Some(api.clone());
        let result = install_pyrun_extension(api, options);
        Box::pin(async move { result })
            as pi_coding_agent::core::extensions::types::BoxFuture<
                'static,
                Result<(), pi_coding_agent::core::extensions::types::HandlerError>,
            >
    });
    let extension = load_extension_from_factory(
        factory,
        &JsString::from(&cwd),
        event_bus(),
        runtime.clone(),
        &JsString::from("pyrun"),
    )
    .await
    .unwrap();
    let session = Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&JsString::from(&cwd)), None).unwrap(),
    ));
    let registry = model_registry().await;
    let mut runner = ExtensionRunner::new(vec![extension], runtime, cwd, session, registry, None);
    runner.set_ui_context(
        Some(Arc::new(TestUi {
            records: Arc::clone(&records),
        })),
        ExtensionMode::Tui,
    );
    let mut actions = actions();
    let records_for_entries = Arc::clone(&records);
    actions.append_entry = Arc::new(move |custom_type, data| {
        records_for_entries
            .lock()
            .unwrap()
            .entries
            .push((custom_type.into(), data));
        Ok(())
    });
    let records_for_user = Arc::clone(&records);
    actions.send_user_message = Arc::new(move |content, _| {
        records_for_user.lock().unwrap().user_messages.push(content);
        Ok(())
    });
    let records_for_calls = Arc::clone(&records);
    actions.call_tool = Arc::new(move |name, params, _, _| {
        records_for_calls
            .lock()
            .unwrap()
            .calls
            .push((name.clone(), params.clone()));
        Box::pin(async move {
            Ok(AgentToolResult {
                content: vec![TextOrImageContent::Text(TextContent::new("tool-output"))],
                details: Some(json!({"tool": name})),
                is_error: None,
                terminate: None,
            })
        })
    });
    let mut context_actions = context_actions();
    let compacted_for_context = Arc::clone(&compacted);
    context_actions.compact = Arc::new(move |_| compacted_for_context.store(true, Ordering::SeqCst));
    let restarted_for_context = Arc::clone(&restarted);
    context_actions.restart = Arc::new(move |_| {
        restarted_for_context.store(true, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    });
    runner.bind_core(actions, context_actions, None).unwrap();
    Harness {
        runner,
        api: captured_api.lock().unwrap().take().expect("api captured"),
        dispatcher: captured.lock().unwrap().take().expect("dispatcher captured"),
        records,
        compacted,
        restarted,
    }
}

#[tokio::test]
async fn registers_pyrun_eval_with_the_canonical_schema() {
    let harness = harness().await;
    let tool = harness.runner.get_tool_definition("pyrun_eval").expect("tool");
    assert_eq!(tool.label, "Pyrun Eval");
    assert_eq!(tool.approval_required, Some(true));
    assert_eq!(
        tool.prompt_snippet.as_ref(),
        Some(&JsString::from(
            "Evaluate Python through the canonical Pyrun JSONL runtime adapter"
        ))
    );
    assert_eq!(tool.prompt_guidelines.as_ref().unwrap().len(), 18);
    assert!(tool.parameters["properties"].get("code").is_some());
    assert!(tool.parameters["properties"].get("session_id").is_some());
    assert_eq!(tool.parameters["required"], serde_json::json!(["code"]));
}

#[tokio::test]
async fn dispatches_builtin_pi_requests() {
    let harness = harness().await;
    let dispatcher = Arc::clone(&harness.dispatcher);
    let ctx = harness.runner.create_context();
    let result = dispatcher(
        PiRequest {
            method: "tools.call".into(),
            params: json!({"name": "read", "params": {"path": "x"}}),
        },
        ctx.clone(),
        None,
        Some("call-1".into()),
    )
    .await
    .unwrap()
    .expect("handled");
    assert_eq!(result["content"][0]["text"], json!("tool-output"));
    assert_eq!(
        harness.records.lock().unwrap().calls,
        vec![("read".into(), json!({"path": "x"}))]
    );

    let result = dispatcher(
        PiRequest {
            method: "commands.run".into(),
            params: json!({"name": "goal", "args": "status"}),
        },
        ctx.clone(),
        None,
        None,
    )
    .await
    .unwrap()
    .expect("handled");
    assert_eq!(result, json!({"name": "goal", "args": "status"}));

    let result = dispatcher(
        PiRequest {
            method: "commands.list".into(),
            params: json!({}),
        },
        ctx.clone(),
        None,
        None,
    )
    .await
    .unwrap()
    .expect("handled");
    assert_eq!(result[0]["name"], json!("goal"));

    let result = dispatcher(
        PiRequest {
            method: "compact".into(),
            params: json!({"customInstructions": "short"}),
        },
        ctx.clone(),
        None,
        None,
    )
    .await
    .unwrap()
    .expect("handled");
    assert_eq!(result, json!({"started": true}));
    assert!(harness.compacted.load(Ordering::SeqCst));

    let result = dispatcher(
        PiRequest {
            method: "messages.enqueue".into(),
            params: json!({"message": "hello", "deliverAs": "followUp"}),
        },
        ctx.clone(),
        None,
        None,
    )
    .await
    .unwrap()
    .expect("handled");
    assert_eq!(result, json!({"enqueued": true}));
    assert_eq!(
        harness.records.lock().unwrap().user_messages,
        vec![UserMessageContent::Text("hello".into())]
    );

    let result = dispatcher(
        PiRequest {
            method: "restart".into(),
            params: json!("notice"),
        },
        ctx.clone(),
        None,
        None,
    )
    .await
    .unwrap()
    .expect("handled");
    assert_eq!(result, json!({"started": true}));
    assert!(harness.restarted.load(Ordering::SeqCst));

    let error = dispatcher(
        PiRequest {
            method: "unknown.method".into(),
            params: json!({}),
        },
        ctx,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(error.to_string(), "Pi capability is unavailable: unknown.method");
}

#[test]
fn validates_dispatch_parameters_like_the_source() {
    let harness = futures_lite_block_on(harness());
    let dispatcher = Arc::clone(&harness.dispatcher);
    let ctx = harness.runner.create_context();
    let error = futures_lite_block_on(dispatcher(
        PiRequest {
            method: "tools.call".into(),
            params: json!({"name": "  "}),
        },
        ctx.clone(),
        None,
        None,
    ))
    .unwrap_err();
    assert_eq!(error.to_string(), "pi.tools.call requires a non-empty tool name");

    let error = futures_lite_block_on(dispatcher(
        PiRequest {
            method: "models.set".into(),
            params: json!({"provider": "p", "id": "i", "thinkingLevel": "nope"}),
        },
        ctx,
        None,
        None,
    ))
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "pi.models.set thinkingLevel must be off, minimal, low, medium, high, xhigh, max, or ultra"
    );
}

fn futures_lite_block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
fn tool_definition_prepares_a_uuidv7_agent_id() {
    let evaluate: pi_coding_agent::bundled_extensions::pyrun::index::PyrunEvaluate = Arc::new(|_, _, _, _, _| {
        Box::pin(async {
            Ok(AgentToolResult {
                content: Vec::new(),
                details: None,
                is_error: None,
                terminate: None,
            })
        })
    });
    let tool = create_pyrun_tool_definition(evaluate, Default::default());
    let prepared = futures_lite_block_on((tool.prepare_execution.as_ref().unwrap())(
        "call".into(),
        None,
        Some(futures_lite_block_on(harness()).runner.create_context()),
    ))
    .unwrap()
    .unwrap();
    assert!(!prepared.agent_id.is_empty());
}

#[tokio::test]
async fn pi_request_undefined_and_null_results_match_typescript_wire_format() {
    let dir = temp_dir("pi-request-null");
    write_runner(
        &dir,
        "fake-pyrun",
        r#"
import json, sys
sys.stdin.readline()
print(json.dumps({"type": "pi_request", "method": "first", "params": {}}), flush=True)
first = json.loads(sys.stdin.readline())
print(json.dumps({"type": "pi_request", "method": "second", "params": {}}), flush=True)
second = json.loads(sys.stdin.readline())
print(json.dumps({"type": "completed", "executed": "x", "value": {"first": first, "second": second}}), flush=True)
"#,
    );
    let runner = runner_for(&dir);
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let handler = Arc::new(move |_request: PiRequest| {
        let calls = Arc::clone(&calls);
        Box::pin(async move {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 { Ok(None) } else { Ok(Some(Value::Null)) }
        })
            as pi_coding_agent::core::extensions::types::BoxFuture<
                'static,
                Result<Option<Value>, pi_coding_agent::core::extensions::types::HandlerError>,
            >
    });
    let result = runner.evaluate(params("x"), None, None, Some(handler)).await.unwrap();
    assert_eq!(
        result.value,
        Some(json!({"first": {}, "second": {"result": null}})),
        "undefined omits the result key; null keeps it"
    );
}

#[tokio::test]
async fn custom_pi_request_handlers_treat_null_as_handled() {
    let harness = harness().await;
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let first_calls = Arc::clone(&calls);
    let first: pi_coding_agent::bundled_extensions::pyrun::eval_tool::PyrunPiRequestDispatcher =
        Arc::new(move |_request, _ctx, _signal, _tool_call_id| {
            first_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(None) })
                as pi_coding_agent::core::extensions::types::BoxFuture<
                    'static,
                    Result<Option<Value>, pi_coding_agent::core::extensions::types::HandlerError>,
                >
        });
    let second_calls = Arc::clone(&calls);
    let second: pi_coding_agent::bundled_extensions::pyrun::eval_tool::PyrunPiRequestDispatcher =
        Arc::new(move |_request, _ctx, _signal, _tool_call_id| {
            second_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(Some(Value::Null)) })
                as pi_coding_agent::core::extensions::types::BoxFuture<
                    'static,
                    Result<Option<Value>, pi_coding_agent::core::extensions::types::HandlerError>,
                >
        });
    let dispatcher = create_pyrun_pi_dispatcher(
        &harness.api,
        &PyrunExtensionOptions {
            pi_request_handlers: vec![first, second]
                .into_iter()
                .map(
                    |dispatch| pi_coding_agent::bundled_extensions::pyrun::index::PyrunPiRequestHandler {
                        dispatch,
                        dispose: None,
                    },
                )
                .collect(),
            ..Default::default()
        },
    );
    let ctx = harness.runner.create_context();
    let result = dispatcher(
        PiRequest {
            method: "custom.method".into(),
            params: json!({}),
        },
        ctx,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(result, Some(Value::Null), "null is a handled result");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "undefined falls through to the next handler"
    );
}
