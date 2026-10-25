#![allow(dead_code)] // Each integration test compiles its own copy and uses a subset.

//! Shared harness for extension runner and discovery tests.

use std::sync::{Arc, Mutex};

use pi_agent_core::harness::ThinkingLevel;
use pi_coding_agent::core::event_bus::{EventBus, EventBusController};
use pi_coding_agent::core::extensions::{
    Extension, ExtensionActions, ExtensionApi, ExtensionContextActions, ExtensionFactory, ExtensionRunner,
    ExtensionRuntime, create_extension_runtime, load_extension_from_factory,
};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::session_manager::{SessionManager, SharedSessionManager};
use pi_js_compat::js_json::{self, JsString, JsValue};
use serde_json::Value;

pub fn event_bus() -> Arc<dyn EventBus> {
    Arc::new(EventBusController::new())
}

pub async fn model_registry() -> Arc<ModelRegistry> {
    Arc::new(
        ModelRegistry::in_memory(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
            serde_json::Map::new(),
        ))
        .unwrap(),
    )
}

pub fn session_manager(cwd: &str) -> SharedSessionManager {
    Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&JsString::from(cwd)), None).expect("in-memory session manager"),
    ))
}

pub fn actions() -> ExtensionActions {
    ExtensionActions {
        send_message: Arc::new(|_, _| Ok(())),
        send_user_message: Arc::new(|_, _| Ok(())),
        append_entry: Arc::new(|_, _| Ok(())),
        request_resume_continuation: Arc::new(|| {}),
        set_session_name: Arc::new(|_| Ok(())),
        get_session_name: Arc::new(|| Ok(None)),
        set_label: Arc::new(|_, _| Ok(())),
        get_active_tools: Arc::new(|| Ok(Vec::new())),
        get_all_tools: Arc::new(|| Ok(Vec::new())),
        set_active_tools: Arc::new(|_| Ok(())),
        call_tool: Arc::new(|_, _, _, _| {
            Box::pin(async {
                Ok(pi_agent_core::AgentToolResult {
                    content: Vec::new(),
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        call_command: Arc::new(|_, _| Box::pin(async { Ok(JsValue::Null) })),
        refresh_tools: Arc::new(|| Ok(())),
        get_commands: Arc::new(|| Ok(Vec::new())),
        set_model: Arc::new(|_| Box::pin(async { Ok(false) })),
        get_thinking_level: Arc::new(|| Ok(ThinkingLevel::Off)),
        set_thinking_level: Arc::new(|_| Ok(())),
    }
}

pub fn context_actions() -> ExtensionContextActions {
    ExtensionContextActions {
        get_model: Arc::new(|| None),
        get_thinking_level: None,
        set_thinking_level: None,
        get_scoped_models: None,
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

pub fn runtime() -> ExtensionRuntime {
    create_extension_runtime()
}

pub async fn extension(
    runtime: &ExtensionRuntime,
    cwd: &str,
    path: &str,
    setup: impl FnOnce(&ExtensionApi) + Send + 'static,
) -> Extension {
    let setup = Mutex::new(Some(setup));
    let factory: ExtensionFactory = Arc::new(move |api| {
        let setup = setup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .expect("extension setup runs once");
        setup(api);
        Box::pin(async { Ok(()) })
    });
    load_extension_from_factory(
        factory,
        &JsString::from(cwd),
        event_bus(),
        runtime.clone(),
        &JsString::from(path),
    )
    .await
    .expect("extension factory succeeds")
}

pub fn normalize_string(value: &str, temp_dir: &str) -> String {
    value.replace(temp_dir, "<tmp>")
}

pub fn normalize(value: &Value, temp_dir: &str) -> Value {
    normalize_with_token(value, temp_dir, "<tmp>")
}

pub fn normalize_with_token(value: &Value, path: &str, token: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(path, token)),
        Value::Array(entries) => Value::Array(
            entries
                .iter()
                .map(|entry| normalize_with_token(entry, path, token))
                .collect(),
        ),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, entry)| (key.clone(), normalize_with_token(entry, path, token)))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub fn log(logs: &Arc<Mutex<Vec<String>>>, message: impl Into<String>) {
    logs.lock().unwrap().push(message.into());
}

pub fn custom_message_with(
    role: &str,
    custom_type: &str,
    content: &str,
    timestamp: i64,
) -> pi_agent_core::AgentMessage {
    js_json::from_value(&pi_js_compat::js_json!({
        "role": role,
        "customType": custom_type,
        "content": content,
        "display": true,
        "timestamp": timestamp,
    }))
    .expect("custom agent message")
}

pub fn runner_for(
    extensions: Vec<Extension>,
    cwd: &str,
    runtime: ExtensionRuntime,
    registry: Arc<ModelRegistry>,
) -> ExtensionRunner {
    ExtensionRunner::new(extensions, runtime, cwd, session_manager(cwd), registry, None)
}

pub fn runner_for_session(
    cwd: &str,
    runtime: ExtensionRuntime,
    session_manager: SharedSessionManager,
    registry: Arc<ModelRegistry>,
) -> ExtensionRunner {
    ExtensionRunner::new(Vec::new(), runtime, cwd, session_manager, registry, None)
}

pub fn bind(runner: &mut ExtensionRunner) {
    runner.bind_core(actions(), context_actions(), None).unwrap();
}
