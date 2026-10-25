//! Differential tests for the bundled `loop`, `codex-fast`, and `effort`
//! extensions against the pinned TypeScript modules.

use pi_js_compat::js_json::JsString;
#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use std::sync::Arc;

use bundled::{Harness, HarnessOptions, test_model};
use oracle::TsOracle;
use pi_agent_core::harness::ThinkingLevel;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::extensions::types::{
    BeforeProviderRequestEvent, ExtensionEvent, ModelSelectEvent, NotifyType, SessionStartEvent, SessionStartReason,
    ThinkingLevelSelectEvent,
};
use pi_coding_agent::core::extensions::{
    Extension, ExtensionRuntime, create_extension_runtime, load_extension_from_factory,
};
use pi_coding_agent::core::system_prompt::BuildSystemPromptOptions;
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

const LOOP_SOURCE: &str = "packages/coding-agent/extensions/loop/src/index.ts";

fn notification_value(notifications: &[(JsString, Option<NotifyType>)]) -> Value {
    Value::Array(
        notifications
            .iter()
            .map(|(message, notify_type)| {
                json!([
                    message,
                    notify_type.map(|notify_type| match notify_type {
                        NotifyType::Info => "info",
                        NotifyType::Warning => "warning",
                        NotifyType::Error => "error",
                    })
                ])
            })
            .collect(),
    )
}

fn status_value(statuses: &[(JsString, Option<JsString>)]) -> Value {
    Value::Array(statuses.iter().map(|(key, value)| json!([key, value])).collect())
}

async fn load_loop(runtime: &ExtensionRuntime) -> Extension {
    load_extension_from_factory(
        Arc::new(|pi| {
            let result = pi_coding_agent::bundled_extensions::r#loop::loop_extension(pi);
            Box::pin(async move { result })
        }),
        &JsString::from("/repo"),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        runtime.clone(),
        &JsString::from("<first-party:loop>"),
    )
    .await
    .expect("loop extension")
}

async fn load_effort(runtime: &ExtensionRuntime) -> Extension {
    load_extension_from_factory(
        Arc::new(|pi| {
            let result = pi_coding_agent::bundled_extensions::effort::effort_extension(pi);
            Box::pin(async move { result })
        }),
        &JsString::from("/repo"),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        runtime.clone(),
        &JsString::from("<first-party:effort>"),
    )
    .await
    .expect("effort extension")
}

fn loop_metadata(extension: &Extension) -> Value {
    let tool = extension
        .read()
        .tools
        .get("loop")
        .expect("loop tool")
        .definition
        .clone();
    json!({
        "toolName": tool.name,
        "toolLabel": tool.label,
        "description": tool.description,
        "promptSnippet": tool.prompt_snippet,
        "promptGuidelines": tool.prompt_guidelines,
        "approvalRequired": tool.approval_required,
        "parameters": tool.parameters,
        "commandDescription": extension.read().commands.get(&pi_js_compat::js_json::JsString::from("loop")).and_then(|command| command.description.clone()),
    })
}

async fn rust_loop_results(cases: &[Value]) -> Vec<Value> {
    let mut results = Vec::new();
    for case in cases {
        let runtime = create_extension_runtime();
        let extension = load_loop(&runtime).await;
        let harness = Harness::new(vec![extension.clone()], runtime.clone(), HarnessOptions::new("/repo")).await;
        let name = case.get("name").cloned().unwrap_or(Value::Null);
        let metadata = loop_metadata(&extension);
        if let Some(command) = case.get("command").and_then(Value::as_string) {
            let handler = extension
                .read()
                .commands
                .get(&pi_js_compat::js_json::JsString::from("loop"))
                .expect("loop command")
                .handler
                .clone();
            handler(command.to_owned(), harness.command_context())
                .await
                .expect("loop command");
            results.push(json!({
                "name": name,
                "metadata": metadata,
                "notifications": notification_value(&harness.ui.notifications()),
                "editor": harness.ui.editor_texts.lock().unwrap().clone(),
                "sends": [],
            }));
            continue;
        }
        let tool = extension
            .read()
            .tools
            .get("loop")
            .expect("loop tool")
            .definition
            .clone();
        let params = case.get("tool").cloned().unwrap_or(Value::Null);
        let result = (tool.execute)("loop-call".to_owned(), params, None, None, Some(harness.context()))
            .await
            .expect("loop tool");
        let text: Vec<Value> = result
            .content
            .iter()
            .map(|content| match content {
                TextOrImageContent::Text(text) => Value::String(text.text.clone()),
                TextOrImageContent::Image(_) => Value::Null,
            })
            .collect();
        results.push(json!({
            "name": name,
            "metadata": metadata,
            "text": text,
            "details": result.details,
            "notifications": notification_value(&harness.ui.notifications()),
            "editor": harness.ui.editor_texts.lock().unwrap().clone(),
            "sends": [],
        }));
    }
    results
}

#[tokio::test(start_paused = true)]
async fn differential_loop_registration_and_handlers() {
    let cases = vec![
        json!({"name": "status-empty", "command": "status"}),
        json!({"name": "stop-empty", "command": "stop"}),
        json!({"name": "off-empty", "command": "off"}),
        json!({"name": "invalid-usage", "command": "bogus"}),
        json!({"name": "missing-prompt", "command": "5s"}),
        json!({"name": "start-command", "command": "10s check progress"}),
        json!({"name": "start-ms", "command": "1500ms check progress"}),
        json!({"name": "start-hours", "command": "2h wait"}),
        json!({"name": "tool-status", "tool": {"action": "status"}}),
        json!({"name": "tool-stop", "tool": {"action": "stop"}}),
        json!({"name": "tool-start-invalid", "tool": {"action": "start", "intervalSeconds": 0.5, "prompt": "x"}}),
        json!({"name": "tool-start-missing-prompt", "tool": {"action": "start", "intervalSeconds": 3}}),
        json!({"name": "tool-start", "tool": {"action": "start", "intervalSeconds": 3, "prompt": "continue"}}),
        json!({"name": "tool-unknown-action", "tool": {"action": "bogus"}}),
    ];
    let mut ts = TsOracle::spawn("bundled-extensions-a-oracle.ts", LOOP_SOURCE);
    let response = ts.request(&json!({"id": 1, "operation": "loop", "cases": cases}));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "oracle failed: {response:?}"
    );
    let expected: Vec<Value> =
        pi_js_compat::js_json::from_value(&response["output"]["results"].clone()).expect("oracle results");
    let actual = rust_loop_results(&cases).await;
    assert_eq!(actual, expected);
}

fn fast_model(case: &Value) -> Option<pi_ai::types::Model> {
    let model = case.get("model")?;
    if model.is_null() {
        return None;
    }
    let mut test_model = test_model(model.get("reasoning").and_then(Value::as_bool).unwrap_or(true));
    test_model.id = model
        .get("modelId")
        .and_then(Value::as_string)
        .map(|tag| tag.to_utf8().expect("scalar model tag"))
        .unwrap_or_else(|| "test-model".to_owned());
    test_model.provider = model
        .get("provider")
        .and_then(Value::as_string)
        .map(|tag| tag.to_utf8().expect("scalar model tag"))
        .unwrap_or_else(|| "openai-codex".to_owned());
    test_model.api = model
        .get("api")
        .and_then(Value::as_string)
        .map(|tag| tag.to_utf8().expect("scalar model tag"))
        .unwrap_or_else(|| "openai-codex-responses".to_owned());
    Some(test_model)
}

fn fast_session(case: &Value) -> pi_coding_agent::core::session_manager::SharedSessionManager {
    let mut session =
        pi_coding_agent::core::session_manager::SessionManager::in_memory(Some(&JsString::from("/repo")), None)
            .expect("session");
    if let Some(branch) = case.get("branch").and_then(Value::as_array) {
        for entry in branch {
            session
                .append_custom_entry(
                    &entry
                        .get("customType")
                        .and_then(Value::as_string)
                        .cloned()
                        .unwrap_or_default(),
                    entry.get("data").cloned(),
                )
                .expect("custom entry");
        }
    }
    Arc::new(std::sync::Mutex::new(session))
}

async fn rust_fast_results(cases: &[Value]) -> Vec<Value> {
    let mut results = Vec::new();
    for case in cases {
        let runtime = create_extension_runtime();
        let authority = pi_coding_agent::bundled_extensions::codex_fast::FastModeAuthority::new();
        if let Some(tier) = case.get("tier").and_then(Value::as_string) {
            authority.set_service_tier(Some(match tier {
                kind if kind == "ultra" => pi_coding_agent::bundled_extensions::codex_fast::FastServiceTier::Ultrafast,
                _ => pi_coding_agent::bundled_extensions::codex_fast::FastServiceTier::Priority,
            }));
        }
        let options = pi_coding_agent::bundled_extensions::codex_fast::CodexFastOptions {
            authority: authority.clone(),
        };
        let extension = load_extension_from_factory(
            Arc::new(move |pi| {
                let options = options.clone();
                let result = pi_coding_agent::bundled_extensions::codex_fast::codex_fast_extension(pi, &options);
                Box::pin(async move { result })
            }),
            &JsString::from("/repo"),
            Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
            runtime.clone(),
            &JsString::from("<first-party:codex-fast>"),
        )
        .await
        .expect("codex-fast extension");

        let mut options = HarnessOptions::new("/repo");
        options.model = fast_model(case);
        options.session_manager = fast_session(case);
        options.settings = case.get("settings").and_then(Value::as_object).cloned();
        options.multi_agent_agent_id = case
            .get("child")
            .and_then(Value::as_bool)
            .filter(|child| *child)
            .map(|_| JsString::from("child-agent"));
        let harness = Harness::new(vec![extension.clone()], runtime, options).await;

        let mut handler_result = Value::Null;
        for op in case.get("ops").and_then(Value::as_array).cloned().unwrap_or_default() {
            match op.get("kind").and_then(Value::as_string).cloned().unwrap_or_default() {
                kind if kind == "command" => {
                    let handler = extension
                        .read()
                        .commands
                        .get(&pi_js_compat::js_json::JsString::from("fast"))
                        .expect("fast command")
                        .handler
                        .clone();
                    handler(
                        op.get("args").and_then(Value::as_string).cloned().unwrap_or_default(),
                        harness.command_context(),
                    )
                    .await
                    .expect("fast command");
                }
                kind if kind == "session_start" => {
                    let mut event = ExtensionEvent::SessionStart(SessionStartEvent {
                        reason: SessionStartReason::Resume,
                        previous_session_file: None,
                    });
                    harness.runner.emit(&mut event).await;
                }
                kind if kind == "model_select" => {
                    let model = op
                        .get("model")
                        .and_then(|model| fast_model(&json!({"model": model})))
                        .expect("model_select model");
                    let mut event = ExtensionEvent::ModelSelect(Box::new(ModelSelectEvent {
                        model,
                        previous_model: None,
                        source: pi_coding_agent::core::extensions::types::ModelSelectSource::Set,
                    }));
                    harness.runner.emit(&mut event).await;
                }
                kind if kind == "before_provider_request" => {
                    let handler = extension
                        .read()
                        .handlers
                        .before_provider_request
                        .first()
                        .expect("before_provider_request handler")
                        .clone();
                    let payload = op.get("payload").cloned().unwrap_or(Value::Null);
                    let result = handler
                        .call(
                            BeforeProviderRequestEvent {
                                payload: payload.clone(),
                            },
                            harness.context(),
                        )
                        .await
                        .expect("before_provider_request");
                    handler_result = result.unwrap_or(Value::Null);
                }
                other => panic!("unknown fast op {other:?}"),
            }
        }

        results.push(json!({
            "name": case.get("name").cloned().unwrap_or(Value::Null),
            "commandDescription": extension.read().commands.get(&pi_js_compat::js_json::JsString::from("fast")).and_then(|command| command.description.clone()),
            "serviceTier": authority.service_tier().map(|tier| tier.as_str()),
            "notifications": notification_value(&harness.ui.notifications()),
            "statuses": status_value(&harness.ui.status_calls.lock().unwrap()),
            "editor": harness.ui.editor_texts.lock().unwrap().clone(),
            "entries": harness.actions.entries.lock().unwrap().iter().map(|(custom_type, data)| json!([custom_type, data])).collect::<Vec<_>>(),
            "handlerResult": handler_result,
        }));
    }
    results
}

#[tokio::test(start_paused = true)]
async fn differential_codex_fast_handlers() {
    let cases = vec![
        json!({"name": "team-provider", "model": {"provider": "openai-codex-team"}, "ops": [{"kind": "command", "args": "ultra"}, {"kind": "before_provider_request", "payload": {"model": "test-model", "service_tier": "default"}}]}),
        json!({"name": "invalid-command", "model": {}, "ops": [{"kind": "command", "args": "invalid"}]}),
        json!({"name": "missing-model", "model": null, "ops": [{"kind": "command", "args": "on"}]}),
        json!({"name": "persisted-null-over-config", "model": {}, "settings": {"defaultCodexFastMode": "priority"}, "branch": [{"customType": "codex-fast-mode", "data": {"serviceTier": "ultrafast"}}, {"customType": "codex-fast-mode", "data": {"serviceTier": null}}, {"customType": "codex-fast-mode", "data": {"serviceTier": "invalid"}}], "ops": [{"kind": "session_start"}]}),
        json!({"name": "toggle", "model": {}, "ops": [{"kind": "command", "args": ""}, {"kind": "command", "args": ""}]}),
        json!({"name": "explicit", "model": {"provider": "openai-codex-gc"}, "ops": [{"kind": "command", "args": "on"}, {"kind": "command", "args": "off"}]}),
        json!({"name": "ultra", "model": {}, "ops": [{"kind": "command", "args": "ultra"}, {"kind": "before_provider_request", "payload": {"model": "test-model"}}, {"kind": "command", "args": "off"}, {"kind": "before_provider_request", "payload": {"model": "test-model"}}]}),
        json!({"name": "unsupported", "model": {"provider": "anthropic", "api": "anthropic-messages"}, "ops": [{"kind": "command", "args": "on"}]}),
        json!({"name": "non-object-payload", "model": {}, "ops": [{"kind": "command", "args": "on"}, {"kind": "before_provider_request", "payload": "unexpected"}]}),
        json!({"name": "child", "model": {}, "child": true, "ops": [{"kind": "command", "args": "off"}]}),
        json!({"name": "restore-ultrafast", "model": {}, "branch": [{"customType": "codex-fast-mode", "data": {"serviceTier": "ultrafast"}}], "ops": [{"kind": "session_start"}]}),
        json!({"name": "restore-configured", "model": {}, "settings": {"defaultCodexFastMode": "priority"}, "ops": [{"kind": "session_start"}]}),
        json!({"name": "model-switch", "model": {}, "ops": [{"kind": "command", "args": "on"}, {"kind": "model_select", "model": {"provider": "anthropic", "api": "anthropic-messages"}}, {"kind": "model_select", "model": {}}]}),
        json!({"name": "openai-provider", "model": {"provider": "openai", "api": "openai-responses"}, "ops": [{"kind": "command", "args": "on"}]}),
    ];
    let mut ts = TsOracle::spawn("bundled-extensions-a-oracle.ts", LOOP_SOURCE);
    let response = ts.request(&json!({"id": 2, "operation": "fast", "cases": cases}));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "oracle failed: {response:?}"
    );
    let expected: Vec<Value> =
        pi_js_compat::js_json::from_value(&response["output"]["results"].clone()).expect("oracle results");
    let actual = rust_fast_results(&cases).await;
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn codex_fast_status_reads_current_authority_and_display_override() {
    use pi_coding_agent::bundled_extensions::codex_fast::{
        CodexFastOptions, FastModeAuthority, FastServiceTier, codex_fast_extension,
    };
    use pi_coding_agent::core::footer_data_provider::FooterSessionOverride;

    let runtime = create_extension_runtime();
    let authority = FastModeAuthority::new();
    let shared = authority.clone();
    let extension = load_extension_from_factory(
        Arc::new(move |pi| {
            let result = codex_fast_extension(
                pi,
                &CodexFastOptions {
                    authority: shared.clone(),
                },
            );
            Box::pin(async move { result })
        }),
        &"/repo".into(),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        runtime.clone(),
        &"<first-party:codex-fast>".into(),
    )
    .await
    .expect("extension");
    let mut options = HarnessOptions::new("/repo");
    options.model = fast_model(&json!({"model": {}}));
    let harness = Harness::new(vec![extension], runtime, options).await;
    let mut event = ExtensionEvent::SessionStart(SessionStartEvent {
        reason: SessionStartReason::Resume,
        previous_session_file: None,
    });
    harness.runner.emit(&mut event).await;
    let status = harness
        .ui
        .status_values
        .lock()
        .unwrap()
        .get(&JsString::from("codex-fast"))
        .cloned()
        .flatten()
        .expect("status callback remains registered when off");
    assert_eq!(status.evaluate(None).expect("off"), None);
    authority.set_service_tier(Some(FastServiceTier::Ultrafast));
    assert_eq!(
        status.evaluate(None).expect("live authority"),
        Some("fast ultra".into())
    );
    let mut view = FooterSessionOverride {
        cwd: "/repo".into(),
        session_manager: None,
        model: fast_model(&json!({"model": {"provider": "anthropic"}})),
        thinking_level: "off".into(),
        context_usage: None,
    };
    assert_eq!(status.evaluate(Some(view.clone())).expect("unsupported display"), None);
    view.model = fast_model(&json!({"model": {"provider": "openai-codex-team"}}));
    assert_eq!(
        status.evaluate(Some(view.clone())).expect("team display"),
        Some("fast ultra".into())
    );
    view.model = None;
    assert_eq!(status.evaluate(Some(view)).expect("missing display model"), None);
    authority.set_service_tier(None);
    assert_eq!(status.evaluate(None).expect("cleared authority"), None);
}

fn effort_model(case: &Value) -> Option<pi_ai::types::Model> {
    let model = case.get("model")?;
    if model.is_null() {
        return None;
    }
    Some(test_model(
        model.get("reasoning").and_then(Value::as_bool).unwrap_or(true),
    ))
}

fn effort_session(case: &Value) -> pi_coding_agent::core::session_manager::SharedSessionManager {
    let mut session =
        pi_coding_agent::core::session_manager::SessionManager::in_memory(Some(&JsString::from("/repo")), None)
            .expect("session");
    if let Some(branch) = case.get("branch").and_then(Value::as_array) {
        for entry in branch {
            session
                .append_custom_entry(
                    &entry
                        .get("customType")
                        .and_then(Value::as_string)
                        .cloned()
                        .unwrap_or_default(),
                    entry.get("data").cloned(),
                )
                .expect("custom entry");
        }
    }
    Arc::new(std::sync::Mutex::new(session))
}

async fn rust_effort_results(cases: &[Value]) -> Vec<Value> {
    let theme_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = load_theme_from_path(
        &JsString::from(theme_path.to_str().expect("utf-8 test path")),
        Some(ColorMode::Truecolor),
    )
    .expect("dark theme");
    set_theme_instance(Arc::new(theme));

    let mut results = Vec::new();
    for case in cases {
        let runtime = create_extension_runtime();
        let extension = load_effort(&runtime).await;
        let mut options = HarnessOptions::new("/repo");
        options.model = effort_model(case);
        options.session_manager = effort_session(case);
        options.thinking_level = match case.get("thinkingLevel").and_then(Value::as_string) {
            Some(value) if value == "ultra" => ThinkingLevel::Ultra,
            Some(value) if value == "max" => ThinkingLevel::Max,
            Some(value) if value == "high" => ThinkingLevel::High,
            _ => ThinkingLevel::Off,
        };
        options.multi_agent_agent_id = case
            .get("child")
            .and_then(Value::as_bool)
            .filter(|child| *child)
            .map(|_| JsString::from("child-agent"));
        let harness = Harness::new(vec![extension.clone()], runtime, options).await;
        *harness.actions.active_tools.lock().unwrap() = case
            .get("activeTools")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|name| name.as_string().unwrap().to_utf8().unwrap())
            .collect();
        harness.ui.select_result(
            case.get("selectResult")
                .and_then(Value::as_string)
                .map(|text| text.to_utf8().expect("fixture selector choice"))
                .as_deref(),
        );

        let mut system_prompt = Value::Null;
        for op in case.get("ops").and_then(Value::as_array).cloned().unwrap_or_default() {
            match op.get("kind").and_then(Value::as_string).cloned().unwrap_or_default() {
                kind if kind == "command" => {
                    let name = op
                        .get("command")
                        .and_then(Value::as_string)
                        .cloned()
                        .unwrap_or_default();
                    let handler = extension
                        .read()
                        .commands
                        .get(&pi_js_compat::js_json::JsString::from(name))
                        .expect("effort command")
                        .handler
                        .clone();
                    handler(
                        op.get("args").and_then(Value::as_string).cloned().unwrap_or_default(),
                        harness.command_context(),
                    )
                    .await
                    .expect("effort command");
                }
                kind if kind == "session_start" => {
                    let mut event = ExtensionEvent::SessionStart(SessionStartEvent {
                        reason: SessionStartReason::Resume,
                        previous_session_file: None,
                    });
                    harness.runner.emit(&mut event).await;
                }
                kind if kind == "thinking_level_select" => {
                    let mut event = ExtensionEvent::ThinkingLevelSelect(ThinkingLevelSelectEvent {
                        level: match op.get("level").and_then(Value::as_string) {
                            Some(value) if value == "ultra" => ThinkingLevel::Ultra,
                            _ => ThinkingLevel::Max,
                        },
                        previous_level: ThinkingLevel::Max,
                    });
                    harness.runner.emit(&mut event).await;
                }
                kind if kind == "before_agent_start" => {
                    let result = harness
                        .runner
                        .emit_before_agent_start(
                            &JsString::from("hi"),
                            None,
                            &op.get("systemPrompt")
                                .and_then(Value::as_string)
                                .cloned()
                                .unwrap_or_default(),
                            BuildSystemPromptOptions::default(),
                        )
                        .await;
                    system_prompt = result
                        .and_then(|result| result.system_prompt)
                        .map(Value::String)
                        .unwrap_or(Value::Null);
                }
                other => panic!("unknown effort op {other:?}"),
            }
        }

        let select_calls: Vec<Value> = harness
            .ui
            .select_calls
            .lock()
            .unwrap()
            .iter()
            .map(|(title, options)| json!([title, options]))
            .collect();
        let descriptions = {
            let registration = extension.read();
            json!({
                "effort": registration.commands.get(&pi_js_compat::js_json::JsString::from("effort")).and_then(|command| command.description.clone()),
                "multiAgent": registration.commands.get(&pi_js_compat::js_json::JsString::from("multi-agent")).and_then(|command| command.description.clone()),
            })
        };
        results.push(json!({
            "name": case.get("name").cloned().unwrap_or(Value::Null),
            "commandDescriptions": descriptions,
            "thinkingLevel": harness.thinking.lock().unwrap().as_str(),
            "notifications": notification_value(&harness.ui.notifications()),
            "statuses": status_value(&harness.ui.status_calls.lock().unwrap()),
            "editor": harness.ui.editor_texts.lock().unwrap().clone(),
            "entries": harness.actions.entries.lock().unwrap().iter().map(|(custom_type, data)| json!([custom_type, data])).collect::<Vec<_>>(),
            "selectCalls": select_calls,
            "activeTools": harness.actions.active_tools.lock().unwrap().clone(),
            "toolUpdates": harness.actions.tool_updates.lock().unwrap().clone(),
            "systemPrompt": system_prompt,
        }));
    }
    results
}

#[tokio::test(start_paused = true)]
async fn differential_effort_handlers() {
    let cases = vec![
        json!({"name":"disable-tools","model":{},"activeTools":["read","spawn_agent","send_agent_message","spawn_agent","custom"],"ops":[{"kind":"command","command":"multi-agent","args":"disabled"},{"kind":"before_agent_start","systemPrompt":"base\n\n<multi_agent_mode>old policy</multi_agent_mode>"}]}),
        json!({"name":"restore-tools","model":{},"activeTools":["read","spawn_agent","wait_agent"],"ops":[{"kind":"command","command":"multi-agent","args":"disabled"},{"kind":"command","command":"multi-agent","args":"proactive"}]}),
        json!({"name":"disabled-ultra","model":{},"thinkingLevel":"ultra","ops":[{"kind":"command","command":"multi-agent","args":"disabled"}]}),
        json!({"name":"select-active","model":{},"selectResult":"active","ops":[{"kind":"command","command":"multi-agent","args":""}]}),
        json!({"name": "select-high", "model": {}, "selectResult": "high", "ops": [{"kind": "command", "command": "effort", "args": ""}]}),
        json!({"name": "set-high", "model": {}, "thinkingLevel": "off", "ops": [{"kind": "command", "command": "effort", "args": "high"}]}),
        json!({"name": "unsupported", "model": {"reasoning": false}, "ops": [{"kind": "command", "command": "effort", "args": "high"}]}),
        json!({"name": "no-model", "model": null, "ops": [{"kind": "command", "command": "effort", "args": "high"}]}),
        json!({"name": "cancel-select", "model": {}, "ops": [{"kind": "command", "command": "effort", "args": ""}]}),
        json!({"name": "explicit-policy", "model": {}, "ops": [{"kind": "command", "command": "multi-agent", "args": "explicit"}, {"kind": "before_agent_start", "systemPrompt": "base\n\n<multi_agent_mode>Proactive multi-agent delegation is active.</multi_agent_mode>"}]}),
        json!({"name": "restore-explicit", "model": {}, "branch": [{"customType": "multi-agent-mode", "data": {"mode": "explicit"}}], "ops": [{"kind": "session_start"}, {"kind": "before_agent_start", "systemPrompt": "base"}]}),
        json!({"name": "ultra", "model": {}, "thinkingLevel": "high", "ops": [{"kind": "command", "command": "effort", "args": "ultra"}, {"kind": "before_agent_start", "systemPrompt": "base"}]}),
        json!({"name": "explicit-from-ultra", "model": {}, "thinkingLevel": "ultra", "ops": [{"kind": "command", "command": "multi-agent", "args": "explicit"}]}),
        json!({"name": "thinking-ultra", "model": {}, "ops": [{"kind": "command", "command": "multi-agent", "args": "explicit"}, {"kind": "thinking_level_select", "level": "ultra", "previousLevel": "max"}]}),
        json!({"name": "child", "model": {}, "child": true, "ops": [{"kind": "command", "command": "multi-agent", "args": "explicit"}, {"kind": "before_agent_start", "systemPrompt": "base"}]}),
        json!({"name": "status", "model": {}, "ops": [{"kind": "session_start"}]}),
        json!({"name": "invalid-mode", "model": {}, "ops": [{"kind": "command", "command": "multi-agent", "args": "bogus"}]}),
        json!({"name": "selector-mode", "model": {}, "selectResult": "explicit", "ops": [{"kind": "command", "command": "multi-agent", "args": ""}]}),
    ];
    let mut ts = TsOracle::spawn("bundled-extensions-a-oracle.ts", LOOP_SOURCE);
    let response = ts.request(&json!({"id": 3, "operation": "effort", "cases": cases}));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "oracle failed: {response:?}"
    );
    let expected: Vec<Value> =
        pi_js_compat::js_json::from_value(&response["output"]["results"].clone()).expect("oracle results");
    let actual = rust_effort_results(&cases).await;
    assert_eq!(actual, expected);
}
