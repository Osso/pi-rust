#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use bundled::{Harness, HarnessOptions, test_model};
use pi_coding_agent::core::extensions::types::{
    BeforeProviderRequestEvent, ModelSelectEvent, ModelSelectSource, SessionStartEvent, SessionStartReason,
};
use pi_coding_agent::core::extensions::{ExtensionRegistry, create_extension_runtime, load_extension_from_factory};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use std::sync::{Arc, Mutex};

fn model(value: &Value) -> Option<pi_ai::types::Model> {
    let object = value.as_object()?;
    let mut model = test_model(true);
    model.id = object.get("id")?.as_string()?.to_utf8().expect("model id");
    model.provider = object.get("provider")?.as_string()?.to_utf8().expect("provider");
    Some(model)
}

async fn run_case(case: &Value) -> Value {
    let runtime = create_extension_runtime();
    let mut registry = ExtensionRegistry::new();
    pi_coding_agent::bundled_extensions::register_small_extensions(
        &mut registry,
        &pi_coding_agent::bundled_extensions::SmallExtensionsOptions {
            get_debug_repl: Arc::new(|| panic!("debug must not be invoked")),
            multi_agent_store: None,
            fast_mode_authority: Default::default(),
            fetch: None,
        },
    );
    let factory = registry
        .get(&"<first-party:codex-daybreak>".into())
        .expect("frozen daybreak registration")
        .clone();
    let extension = load_extension_from_factory(
        factory,
        &"/repo".into(),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        runtime.clone(),
        &"<first-party:codex-daybreak>".into(),
    )
    .await
    .expect("extension");
    let mut options = HarnessOptions::new("/repo");
    options.model = model(&case["model"]);
    options.multi_agent_agent_id = case.get("child").and_then(Value::as_string).cloned();
    options.multi_agent_requires_agent_id = case["requiresId"].as_bool().unwrap_or(false);
    let mut manager = SessionManager::in_memory(Some(&JsString::from("/repo")), None).expect("session");
    for entry in case.get("branch").and_then(Value::as_array).into_iter().flatten() {
        if entry["type"].as_string().is_some_and(|value| value == "custom") {
            manager
                .append_custom_entry(
                    entry["customType"].as_string().expect("custom type"),
                    entry.get("data").cloned(),
                )
                .expect("entry");
        }
    }
    options.session_manager = Arc::new(Mutex::new(manager));
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let command = extension
        .read()
        .commands
        .get(&JsString::from("daybreak"))
        .expect("command")
        .clone();
    let mut payloads = Vec::new();
    for op in case["ops"].as_array().expect("ops") {
        let handlers = extension.read().handlers.clone();
        match op["kind"]
            .as_string()
            .expect("kind")
            .to_utf8()
            .expect("kind tag")
            .as_str()
        {
            "command" => (command.handler)(
                op.get("args").and_then(Value::as_string).cloned().unwrap_or_default(),
                harness.command_context(),
            )
            .await
            .expect("command"),
            "session_start" => {
                handlers.session_start[0]
                    .call(
                        SessionStartEvent {
                            reason: SessionStartReason::Resume,
                            previous_session_file: None,
                        },
                        harness.context(),
                    )
                    .await
                    .expect("session start");
            }
            "model_select" => {
                handlers.model_select[0]
                    .call(
                        ModelSelectEvent {
                            model: model(&op["model"]).expect("selected model"),
                            previous_model: None,
                            source: ModelSelectSource::Set,
                        },
                        harness.context(),
                    )
                    .await
                    .expect("model select");
            }
            "before_provider_request" => {
                payloads.push(
                    handlers.before_provider_request[0]
                        .call(
                            BeforeProviderRequestEvent {
                                payload: op["payload"].clone(),
                            },
                            harness.context(),
                        )
                        .await
                        .expect("request")
                        .unwrap_or(Value::Null),
                );
            }
            _ => panic!("unknown op"),
        }
    }
    json!({
        "description": command.description,
        "entries": harness.actions.entries.lock().unwrap().iter().map(|(name, data)| json!([name, data])).collect::<Vec<_>>(),
        "notifications": harness.ui.notifications().iter().map(|(text, kind)| json!([text, match kind { Some(pi_coding_agent::core::extensions::types::NotifyType::Warning) => "warning", _ => "info" }])).collect::<Vec<_>>(),
        "statuses": harness.ui.status_calls.lock().unwrap().iter().map(|(key, value)| json!([key, value])).collect::<Vec<_>>(),
        "editor": harness.ui.editor_texts.lock().unwrap().clone(), "payloads": payloads,
    })
}

#[tokio::test]
async fn codex_daybreak_matches_frozen_handlers_and_wire_payloads() {
    let eligible = json!({"id": "gpt-6-sol", "provider": "openai-codex"});
    let cases = vec![
        json!({"model": eligible, "ops": [{"kind": "session_start"}, {"kind": "command", "args": " blue "}, {"kind": "before_provider_request", "payload": {"model": "gpt-6-sol", "access_programs": {"other": "kept", "cyber": "old"}}}, {"kind": "command", "args": ""}, {"kind": "command", "args": "off"}]}),
        json!({"model": {"id": "gpt-6-luna", "provider": "openai-codex-team"}, "ops": [{"kind": "command", "args": "BLUE"}, {"kind": "before_provider_request", "payload": {"model": "gpt-6-luna", "access_programs": []}}]}),
        json!({"model": {"id": "gpt-5.5", "provider": "openai-codex-gc"}, "ops": [{"kind": "command", "args": "blue"}, {"kind": "before_provider_request", "payload": {"model": "gpt-5.6-sol"}}]}),
        json!({"model": eligible, "ops": [{"kind": "command", "args": "on"}, {"kind": "command", "args": " "}]}),
        json!({"model": {"id": "gpt-6.1-sol", "provider": "openai-codex"}, "ops": [{"kind": "command", "args": "blue"}, {"kind": "before_provider_request", "payload": {"model": "gpt-6.1-sol"}}, {"kind": "before_provider_request", "payload": {"model": "gpt-6-sol"}}]}),
        json!({"model": {"id": "gpt-6-sol", "provider": "openai"}, "ops": [{"kind": "command", "args": "blue"}, {"kind": "command", "args": "off"}]}),
        json!({"ops": [{"kind": "command", "args": "blue"}, {"kind": "command", "args": "off"}]}),
        json!({"model": eligible, "child": "child", "ops": [{"kind": "command", "args": "blue"}, {"kind": "session_start"}]}),
        json!({"model": eligible, "child": "", "ops": [{"kind": "command", "args": "off"}]}),
        json!({"model": eligible, "requiresId": true, "ops": [{"kind": "command", "args": "blue"}]}),
        json!({"model": eligible, "ops": [{"kind": "command", "args": "blue"}, {"kind": "before_provider_request", "payload": []}, {"kind": "before_provider_request", "payload": null}, {"kind": "before_provider_request", "payload": {"model": 42}}, {"kind": "model_select", "model": {"id": "gpt-6-astra", "provider": "openai-codex"}}]}),
        json!({"model": eligible, "branch": [{"type": "custom", "customType": "codex-daybreak", "data": {"cyber": "daybreak_blue"}}, {"type": "custom", "customType": "other", "data": {"cyber": null}}, {"type": "custom", "customType": "codex-daybreak", "data": {"cyber": "unknown"}}], "ops": [{"kind": "session_start"}, {"kind": "before_provider_request", "payload": {"model": "gpt-6-sol", "access_programs": {"raw": JsString::from_utf16(vec![0xd800])}}}]}),
        json!({"model": eligible, "branch": [{"type": "custom", "customType": "codex-daybreak", "data": {"cyber": "daybreak_blue"}}, {"type": "custom", "customType": "codex-daybreak", "data": {"cyber": null}}], "ops": [{"kind": "session_start"}, {"kind": "before_provider_request", "payload": {"model": "gpt-6-sol"}}]}),
    ];
    let mut ts = oracle::TsOracle::spawn(
        "codex-daybreak-oracle.ts",
        "packages/coding-agent/extensions/codex-daybreak/src/index.ts",
    );
    let response = ts.request(&json!({"id": 1, "operation": "handlers", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle: {response:?}");
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(run_case(case).await, response["output"][index], "case {index}");
    }
}
