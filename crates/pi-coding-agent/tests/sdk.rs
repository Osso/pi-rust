//! Behavioral tests for `core/sdk.ts` and `core/agent-session-services.ts`.

#[path = "support/extensions.rs"]
mod extensions_support;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::encode_node_utf8;
use std::sync::{Arc, Mutex};

use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, RegisterFauxProviderOptions,
    faux_assistant_message, faux_provider,
};
use pi_coding_agent::core::agent_session::PromptOptions;
use pi_coding_agent::core::agent_session_services::{
    CreateAgentSessionFromServicesOptions, CreateAgentSessionServicesOptions, create_agent_session_from_services,
    create_agent_session_services,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::CreateAgentSessionOptions;
use pi_coding_agent::core::session_manager::SessionManager;
use serde_json::json;

async fn faux_runtime(faux: &FauxProviderHandle) -> ModelRegistry {
    let mut data = serde_json::Map::new();
    data.insert("faux".to_owned(), json!({"type": "api_key", "key": "faux-key"}));
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(data)).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    runtime
}

fn faux() -> FauxProviderHandle {
    faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "faux-1".to_owned(),
            name: Some("Faux Model".into()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(100_000.0),
            max_tokens: Some(4096.0),
        }]),
        ..Default::default()
    })
}

#[tokio::test]
async fn create_agent_session_uses_explicit_model_and_persists_prompt() {
    let temp = tempfile::tempdir().expect("temp dir");
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    faux.set_responses(vec![
        faux_assistant_message("hello", FauxAssistantMessageOptions::default()).into(),
    ]);
    let session_dir = temp.path().join("sessions").display().to_string();
    let mut session_manager = SessionManager::create(
        &JsString::from(&temp.path().display().to_string()),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("session manager");
    let control_db = temp.path().join("control.db").display().to_string();
    session_manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(temp.path().display().to_string())),
        agent_dir: Some(JsString::from(temp.path().display().to_string())),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        ..Default::default()
    })
    .await
    .expect("session");
    assert!(result.model_fallback_message.is_none());
    result
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");
    let file = result.session.session_file().expect("persisted session file");
    let content = std::fs::read_to_string(encode_node_utf8(&file)).expect("session file");
    assert!(
        content.contains("\"role\":\"user\""),
        "user message persisted: {content}"
    );
    assert!(
        content.contains("\"role\":\"assistant\""),
        "assistant message persisted: {content}"
    );
}

#[tokio::test]
async fn default_loader_factories_run_before_persisted_session_settings_restore_like_frozen() {
    use pi_coding_agent::core::permissions::presets::SandboxProfileName;
    use pi_coding_agent::core::resource_loader::InlineExtension;
    use pi_coding_agent::core::settings_manager::SettingsManager;
    let temp = tempfile::tempdir().unwrap();
    let root: JsString = temp.path().to_str().unwrap().into();
    let mut oracle = oracle::TsOracle::spawn("sdk-no-model-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({"id":100,"operation":"resource_order","root":root}));
    let mut settings = SettingsManager::in_memory(Default::default(), Default::default());
    settings.set_session_sandbox_profile(Some(SandboxProfileName::FullAccess));
    let settings = Arc::new(Mutex::new(settings));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_factory = Arc::clone(&seen);
    let factory_settings = Arc::clone(&settings);
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(root.clone()),
        agent_dir: Some(root.clone()),
        model_registry: Some(ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap()),
        session_manager: Some(SessionManager::in_memory(Some(&root), None).unwrap()),
        settings_manager: Some(Arc::clone(&settings)),
        extension_factories: vec![InlineExtension::Factory {
            extension_path: None,
            factory: Arc::new(move |_| {
                seen_factory
                    .lock()
                    .unwrap()
                    .push(factory_settings.lock().unwrap().get_session_sandbox_profile());
                Box::pin(async { Ok(()) })
            }),
        }],
        ..Default::default()
    })
    .await
    .unwrap();
    assert_eq!(
        json!({"id":100,"seen":seen.lock().unwrap().clone(),"final":settings.lock().unwrap().get_session_sandbox_profile()}),
        expected
    );
    result.session.dispose();
}

#[tokio::test]
async fn historical_model_entries_do_not_override_settings_without_persisted_metadata() {
    use pi_coding_agent::core::settings_manager::SettingsManager;
    let temp = tempfile::tempdir().unwrap();
    let root: JsString = temp.path().to_str().unwrap().into();
    let mut oracle = oracle::TsOracle::spawn("sdk-no-model-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({"id":101,"operation":"legacy_model","root":root}));
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".into()),
        models: Some(
            ["preferred", "historical"]
                .into_iter()
                .map(|id| FauxModelDefinition {
                    id: id.into(),
                    name: None,
                    reasoning: None,
                    input: None,
                    cost: None,
                    context_window: None,
                    max_tokens: None,
                })
                .collect(),
        ),
        ..Default::default()
    });
    let registry = faux_runtime(&faux).await;
    let settings = SettingsManager::in_memory(
        serde_json::from_value(json!({"defaultProvider":"faux","defaultModel":"preferred"})).unwrap(),
        Default::default(),
    );
    let mut manager = SessionManager::in_memory(Some(&root), None).unwrap();
    manager
        .append_model_change(&"faux".into(), &"historical".into())
        .unwrap();
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(root.clone()),
        agent_dir: Some(root),
        model_registry: Some(registry),
        settings_manager: Some(Arc::new(Mutex::new(settings))),
        session_manager: Some(manager),
        resource_loader: Some(StaticResourceLoader::new()),
        ..Default::default()
    })
    .await
    .unwrap();
    assert_eq!(
        json!({"id":101,"model":result.session.model().id,"fallback":result.model_fallback_message}),
        expected
    );
    result.session.dispose();
}

#[tokio::test]
async fn no_auth_startup_and_restart_persistence_match_frozen() {
    use pi_coding_agent::core::agent_session_runtime::AgentSessionRuntime;
    use pi_coding_agent::core::agent_session_services::AgentSessionServices;
    use pi_coding_agent::core::settings_manager::SettingsManager;
    let source_root = tempfile::tempdir().unwrap();
    let native_root = tempfile::tempdir().unwrap();
    let mut oracle = oracle::TsOracle::spawn("sdk-no-model-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({"id":99,"root":source_root.path()}));
    let cwd: JsString = native_root.path().to_str().unwrap().into();
    let auth_storage = AuthStorage::in_memory(serde_json::Map::new());
    let model_registry = ModelRegistry::in_memory(auth_storage.clone()).unwrap();
    let settings_manager = Arc::new(Mutex::new(SettingsManager::in_memory(
        Default::default(),
        Default::default(),
    )));
    let resource_loader = StaticResourceLoader::new();
    let mut manager = SessionManager::create(
        &cwd,
        Some(&JsString::from(native_root.path().join("sessions").to_str().unwrap())),
        None,
    )
    .unwrap();
    manager
        .set_metadata_control_db_path(
            Some(&JsString::from(native_root.path().join("control.db").to_str().unwrap())),
            Default::default(),
        )
        .unwrap();
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.clone()),
        agent_dir: Some(cwd.clone()),
        auth_storage: Some(auth_storage.clone()),
        model_registry: Some(model_registry.clone()),
        settings_manager: Some(Arc::clone(&settings_manager)),
        resource_loader: Some(resource_loader.clone()),
        session_manager: Some(manager),
        ..Default::default()
    })
    .await
    .unwrap();
    let entry_types = |session: &pi_coding_agent::core::AgentSession| {
        session
            .session_manager()
            .get_entries()
            .iter()
            .map(|entry| entry.entry_type().to_owned())
            .collect::<Vec<_>>()
    };
    let startup = json!({"model":{"provider":result.session.model().provider,"id":result.session.model().id},"message":result.model_fallback_message.as_ref().map(|message|message.replace(pi_coding_agent::config::get_package_dir().unwrap(),"<package>")),"entries":entry_types(&result.session)});
    let session = result.session.clone();
    let services = AgentSessionServices {
        cwd: cwd.clone(),
        agent_dir: cwd,
        auth_storage,
        model_registry,
        settings_manager,
        resource_loader,
        diagnostics: vec![],
    };
    let runtime = AgentSessionRuntime::new(
        result.session,
        services,
        Arc::new(|_| Box::pin(async { Err("factory must not run".into()) })),
        vec![],
        result.model_fallback_message,
    );
    runtime.set_before_process_restart(Some(Arc::new(|| {
        Box::pin(async { Err("stop after persistence".into()) })
    })));
    let error = runtime.restart(None, true).await.unwrap_err().message;
    let persisted = session
        .session_manager()
        .read_persisted_session_settings()
        .unwrap()
        .unwrap();
    let persisted = json!({"model":persisted.model.map(|model|json!({"provider":model.provider,"modelId":model.model_id})),"thinkingLevel":persisted.thinking_level});
    assert_eq!(
        json!({"id":99,"startup":startup,"persisted":persisted,"entries":entry_types(&session),"error":error}),
        expected
    );
}

#[tokio::test]
async fn create_agent_session_keeps_running_without_models() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap();
    let session_dir = temp.path().join("sessions").display().to_string();
    let mut session_manager = SessionManager::create(
        &JsString::from(&temp.path().display().to_string()),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("session manager");
    let control_db = temp.path().join("control.db").display().to_string();
    session_manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(temp.path().display().to_string())),
        agent_dir: Some(JsString::from(temp.path().display().to_string())),
        model_registry: Some(runtime),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        ..Default::default()
    })
    .await
    .expect("session without a model");
    assert_eq!(
        result.model_fallback_message.as_ref(),
        Some(pi_coding_agent::core::auth_guidance::format_no_models_available_message().unwrap()).as_ref()
    );
    assert_eq!(result.session.model().id, "unknown");
    assert_eq!(
        result.session.thinking_level(),
        pi_agent_core::harness::types::ThinkingLevel::Off
    );
    let error = result
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect_err("prompt requires auth");
    assert!(
        error.message().starts_with("No API key found for the selected model."),
        "error: {:?}",
        error.message()
    );
}

#[tokio::test]
async fn services_and_from_services_create_a_working_session() {
    let temp = tempfile::tempdir().expect("temp dir");
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    faux.set_responses(vec![
        faux_assistant_message(
            vec![
                pi_ai::providers::faux::faux_text("service hello"),
                pi_ai::providers::faux::faux_tool_call(
                    "end_turn",
                    serde_json::Map::from_iter([("reason".to_owned(), serde_json::json!("done"))])
                        .into_iter()
                        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                        .collect::<pi_js_compat::js_json::JsObject>(),
                    Some("call-end".to_owned()),
                ),
            ],
            FauxAssistantMessageOptions::default(),
        )
        .into(),
    ]);
    let services = create_agent_session_services(CreateAgentSessionServicesOptions {
        cwd: JsString::from(temp.path().display().to_string()),
        agent_dir: Some(JsString::from(temp.path().display().to_string())),
        settings_manager: None,
        model_registry: Some(runtime),
        auth_storage: None,
        extension_flag_values: None,
        resource_loader_options: None,
        resource_loader: Some(StaticResourceLoader::new()),
        resource_loader_reload_options: None,
    })
    .await
    .expect("services");
    let session_dir = temp.path().join("sessions").display().to_string();
    let mut session_manager = SessionManager::create(
        &JsString::from(&temp.path().display().to_string()),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("session manager");
    let control_db = temp.path().join("control.db").display().to_string();
    session_manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let result = create_agent_session_from_services(CreateAgentSessionFromServicesOptions {
        no_supervisor: None,
        services,
        session_manager,
        model: Some(model),
        thinking_level: None,
        scoped_models: Vec::new(),
        tools: None,
        exclude_tools: None,
        no_tools: None,
        permission_prompt_tool: None,
        multi_agent_runtime_role: None,
        multi_agent_execution_capability: None,
        custom_tools: Vec::new(),
        base_tools: Vec::new(),
        multi_agent_store: None,
        on_shared_channel_message_delivered: None,
        session_start_event: None,
    })
    .await
    .expect("session");
    result
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");
    assert_eq!(result.session.messages().len(), 3);
}

#[tokio::test]
async fn excluded_tools_are_removed_from_initial_active_names() {
    let temp = tempfile::tempdir().expect("temp dir");
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let session_dir = temp.path().join("sessions").display().to_string();
    let mut session_manager = SessionManager::create(
        &JsString::from(&temp.path().display().to_string()),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("session manager");
    let control_db = temp.path().join("control.db").display().to_string();
    session_manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(temp.path().display().to_string())),
        agent_dir: Some(JsString::from(temp.path().display().to_string())),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        no_tools: Some(pi_coding_agent::core::sdk::NoTools::All),
        ..Default::default()
    })
    .await
    .expect("session");
    assert_eq!(result.session.get_active_tool_names(), vec!["end_turn".to_owned()]);
}

#[tokio::test]
async fn create_agent_session_propagates_path_resolution_errors() {
    let temp = tempfile::tempdir().expect("temp dir");
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let session_dir = temp.path().join("sessions").display().to_string();
    let session_manager = SessionManager::create(
        &JsString::from(&temp.path().display().to_string()),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("session manager");

    let cwd_error = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from("file://otherhost/project")),
        agent_dir: Some(JsString::from(temp.path().display().to_string())),
        model_registry: Some(runtime.clone()),
        model: Some(model.clone()),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        ..Default::default()
    })
    .await;
    assert!(cwd_error.is_err(), "cwd resolve errors must propagate");

    let session_dir = temp.path().join("sessions").display().to_string();
    let session_manager = SessionManager::create(
        &JsString::from(&temp.path().display().to_string()),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("session manager");
    let agent_dir_error = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(temp.path().display().to_string())),
        agent_dir: Some(JsString::from("file://otherhost/agent")),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        ..Default::default()
    })
    .await;
    assert!(agent_dir_error.is_err(), "agent dir resolve errors must propagate");
}

#[path = "support/oracle.rs"]
mod oracle;

#[tokio::test]
async fn stream_function_merges_auth_request_environment_headers_and_cwd_like_frozen() {
    use pi_ai::request::SimpleStreamOptions;
    use pi_ai::types::Context;
    let temp = tempfile::tempdir().unwrap();
    let cwd = JsString::from(temp.path().to_str().unwrap());
    let faux = faux();
    let auth = AuthStorage::in_memory(serde_json::Map::from_iter([(
        "faux".to_owned(),
        json!({"type":"api_key","key":"faux-key","env":{"AUTH":"auth","SHARED":"auth"}}),
    )]));
    let registry = ModelRegistry::in_memory(auth).unwrap();
    scripted_registry::register_scripted_provider(&registry, Arc::clone(&faux.provider));
    let mut model = faux.get_model(None).unwrap();
    model.base_url = "https://opencode.ai/v1".into();
    model.headers = Some(
        [
            ("x-auth".to_owned(), "auth".into()),
            ("x-conflict".to_owned(), "auth".into()),
        ]
        .into_iter()
        .collect(),
    );
    let captured = Arc::new(Mutex::new(None));
    let captured_options = Arc::clone(&captured);
    faux.set_responses(vec![pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
        move |_, options, _, _| {
            *captured_options.lock().unwrap() = options;
            Box::pin(async { Ok(faux_assistant_message("ok", Default::default())) })
        },
    ))]);
    let session = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.clone()),
        agent_dir: Some(cwd.clone()),
        model: Some(model.clone()),
        model_registry: Some(registry),
        settings_manager: Some(Arc::new(Mutex::new(
            pi_coding_agent::core::settings_manager::SettingsManager::in_memory(Default::default(), Default::default()),
        ))),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(SessionManager::in_memory(Some(&cwd), None).unwrap()),
        ..Default::default()
    })
    .await
    .unwrap()
    .session;
    let request = json!({"id":1,"cwd":cwd,"options":{"sessionId":"wire-session","cwd":"ignored-cwd","headers":{"x-conflict":"request","x-request":"request"},"env":{"SHARED":"request","REQUEST":"request"}}});
    let mut oracle = oracle::TsOracle::spawn("sdk-stream-options-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&request);
    let mut options = SimpleStreamOptions::default();
    options.stream.session_id = Some("wire-session".into());
    options.stream.cwd = Some("ignored-cwd".into());
    options.stream.request.headers = Some(
        [
            ("x-conflict".to_owned(), Some("request".into())),
            ("x-request".to_owned(), Some("request".into())),
        ]
        .into_iter()
        .collect(),
    );
    options.stream.request.env = Some(
        [
            ("SHARED".to_owned(), "request".into()),
            ("REQUEST".to_owned(), "request".into()),
        ]
        .into_iter()
        .collect(),
    );
    let stream = (session.agent().stream_fn())(
        model,
        Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        },
        options,
    )
    .await
    .unwrap();
    stream.result().await.unwrap();
    let options = captured.lock().unwrap().clone().unwrap();
    assert_eq!(
        json!({"id":1,"cwd":options.stream.cwd,"headers":options.stream.request.headers,"env":options.stream.request.env,"apiKey":options.stream.request.api_key}),
        expected
    );
}

#[tokio::test]
async fn stream_function_forwards_websocket_connect_timeout_from_settings() {
    use pi_ai::request::SimpleStreamOptions;
    use pi_ai::types::Context;

    let temp = tempfile::tempdir().expect("temp dir");
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let captured = Arc::new(Mutex::new(None));
    let captured_options = Arc::clone(&captured);
    faux.set_responses(vec![pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
        move |_context, options, _state, _model| {
            *captured_options.lock().unwrap() = options;
            Box::pin(async { Ok(faux_assistant_message("ok", FauxAssistantMessageOptions::default())) })
        },
    ))]);
    let mut settings = serde_json::Map::new();
    settings.insert("websocketConnectTimeoutMs".to_owned(), json!(1234));
    let session_dir = temp.path().join("sessions").display().to_string();
    let session_manager = SessionManager::create(
        &JsString::from(&temp.path().display().to_string()),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("session manager");
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(temp.path().display().to_string())),
        agent_dir: Some(JsString::from(temp.path().display().to_string())),
        model_registry: Some(runtime),
        model: Some(model.clone()),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        settings_manager: Some(Arc::new(Mutex::new(
            pi_coding_agent::core::settings_manager::SettingsManager::in_memory(
                settings
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
                Default::default(),
            ),
        ))),
        ..Default::default()
    })
    .await
    .expect("session");

    let stream = (result.session.agent().stream_fn())(
        model,
        Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        },
        SimpleStreamOptions::default(),
    )
    .await
    .expect("stream acquisition");
    stream.result().await.expect("stream result");
    assert_eq!(
        captured
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|options| options.stream.websocket_connect_timeout_ms),
        Some(1234.0)
    );
}

#[tokio::test]
async fn stream_function_fails_when_the_websocket_connect_timeout_setting_is_invalid() {
    use pi_ai::request::SimpleStreamOptions;
    use pi_ai::types::Context;

    let temp = tempfile::tempdir().expect("temp dir");
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let mut settings = serde_json::Map::new();
    settings.insert("websocketConnectTimeoutMs".to_owned(), json!("bad"));
    let session_dir = temp.path().join("sessions").display().to_string();
    let session_manager = SessionManager::create(
        &JsString::from(&temp.path().display().to_string()),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("session manager");
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(temp.path().display().to_string())),
        agent_dir: Some(JsString::from(temp.path().display().to_string())),
        model_registry: Some(runtime),
        model: Some(model.clone()),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        settings_manager: Some(Arc::new(Mutex::new(
            pi_coding_agent::core::settings_manager::SettingsManager::in_memory(
                settings
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
                Default::default(),
            ),
        ))),
        ..Default::default()
    })
    .await
    .expect("session");

    let stream = (result.session.agent().stream_fn())(
        model,
        Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        },
        SimpleStreamOptions::default(),
    )
    .await;
    let failure = stream.err().expect("invalid setting must reject acquisition");
    assert_eq!(failure.message(), "Invalid websocketConnectTimeoutMs setting: \"bad\"");
}

#[tokio::test]
async fn create_agent_session_forwards_observer_coordination_options() {
    let temp = tempfile::tempdir().expect("temp dir");
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let session_dir = temp.path().join("sessions").display().to_string();
    let mut session_manager = SessionManager::create(
        &JsString::from(&temp.path().display().to_string()),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("session manager");
    let control_db = temp.path().join("control.db").display().to_string();
    session_manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let store = std::sync::Arc::new(pi_coding_agent::core::multi_agent_store::MultiAgentStore::new(
        pi_coding_agent::core::multi_agent_store::MultiAgentStoreOptions::default(),
    ));
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(temp.path().display().to_string())),
        agent_dir: Some(JsString::from(temp.path().display().to_string())),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        multi_agent_store: Some(store),
        disable_runtime_coordination_inbound: true,
        ..Default::default()
    })
    .await
    .expect("session");
    assert!(result.session.runtime_coordination_inbound_disabled());
}

#[tokio::test]
async fn create_agent_session_forwards_multi_agent_identity_to_extensions() {
    let temp = tempfile::tempdir().expect("temp dir");
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let session_dir = temp.path().join("sessions").display().to_string();
    let mut session_manager = SessionManager::create(
        &JsString::from(&temp.path().display().to_string()),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("session manager");
    let control_db = temp.path().join("control.db").display().to_string();
    session_manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(temp.path().display().to_string())),
        agent_dir: Some(JsString::from(temp.path().display().to_string())),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        multi_agent_agent_id: Some(JsString::from("agent_child")),
        multi_agent_parent_session_id: Some(JsString::from("parent-session")),
        multi_agent_requires_agent_id: true,
        multi_agent_runtime_role: Some(pi_coding_agent::core::agent_session::MultiAgentRuntimeRole::Child),
        ..Default::default()
    })
    .await
    .expect("session");
    let context = result.session.extension_runner().create_context();
    assert_eq!(
        context.multi_agent_agent_id().expect("agent id"),
        Some(JsString::from("agent_child".to_owned()))
    );
    assert_eq!(
        context.multi_agent_parent_session_id().expect("parent session"),
        Some(JsString::from("parent-session".to_owned()))
    );
    assert!(context.multi_agent_requires_agent_id().expect("requires agent id"));
}

#[tokio::test]
async fn create_agent_session_wires_extension_provider_hooks() {
    use pi_coding_agent::core::extensions::{
        AfterProviderResponseEvent, BoxFuture, ContextEvent, ContextEventResult, ExtensionContext, HandlerError,
        LoadExtensionsResult,
    };

    let temp = tempfile::tempdir().expect("temp dir");
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen_context = Arc::new(Mutex::new(Vec::<String>::new()));

    let context_recorder = Arc::clone(&seen_context);
    faux.set_responses(vec![pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
        move |context, _options, _state, _model| {
            let recorder = Arc::clone(&context_recorder);
            Box::pin(async move {
                let serialized = serde_json::to_string(&context.messages).expect("context serializes");
                recorder.lock().unwrap().push(serialized);
                Ok(faux_assistant_message("hooked", FauxAssistantMessageOptions::default()))
            })
        },
    ))]);

    let extension_runtime = extensions_support::runtime();
    let extension_cwd = temp.path().display().to_string();
    let extension = {
        let logs = Arc::clone(&logs);
        extensions_support::extension(&extension_runtime, &extension_cwd, "<inline:hooks>", move |api| {
            let response_logs = Arc::clone(&logs);
            api.on_after_provider_response(move |event: AfterProviderResponseEvent, _context: ExtensionContext| {
                let logs = Arc::clone(&response_logs);
                Box::pin(async move {
                    logs.lock().unwrap().push(format!("response:{}", event.status));
                    Ok(None)
                }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
            })
            .expect("response handler");

            let context_logs = Arc::clone(&logs);
            api.on_context(move |event: ContextEvent, _context: ExtensionContext| {
                let logs = Arc::clone(&context_logs);
                Box::pin(async move {
                    logs.lock().unwrap().push(format!("context:{}", event.messages.len()));
                    let mut messages = event.messages;
                    messages.push(extensions_support::custom_message_with("custom", "hook", "injected", 7));
                    Ok(Some(ContextEventResult {
                        messages: Some(messages),
                    }))
                }) as BoxFuture<'static, Result<Option<ContextEventResult>, HandlerError>>
            })
            .expect("context handler");
        })
        .await
    };

    let loader = Arc::new(StaticResourceLoader {
        extensions: Some(LoadExtensionsResult {
            extensions: vec![extension],
            errors: Vec::new(),
            runtime: extension_runtime,
        }),
        ..Default::default()
    });
    let session_dir = temp.path().join("sessions").display().to_string();
    let mut session_manager = SessionManager::create(
        &JsString::from(&temp.path().display().to_string()),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("session manager");
    let control_db = temp.path().join("control.db").display().to_string();
    session_manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(temp.path().display().to_string())),
        agent_dir: Some(JsString::from(temp.path().display().to_string())),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(loader),
        session_manager: Some(session_manager),
        ..Default::default()
    })
    .await
    .expect("session");

    result
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");

    let logs = logs.lock().unwrap().clone();
    assert!(
        logs.iter().any(|entry| entry == "response:200"),
        "after_provider_response must observe the provider status: {logs:?}"
    );
    assert!(
        logs.iter().any(|entry| entry == "context:1"),
        "transformContext must see the user message: {logs:?}"
    );
    let contexts = seen_context.lock().unwrap().clone();
    assert!(
        contexts.iter().any(|context| context.contains("injected")),
        "the context patch must reach the provider request: {contexts:?}"
    );
}

#[tokio::test]
async fn custom_provider_response_preserves_header_order_through_sdk_and_json() {
    use pi_coding_agent::core::extensions::{AfterProviderResponseEvent, ExtensionContext, LoadExtensionsResult};
    use pi_coding_agent::core::model_registry::ProviderConfigInput;

    let temp = tempfile::tempdir().unwrap();
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let mut model = faux.get_model(None).unwrap();
    model.provider = "ordered-custom".to_owned();
    let observed = Arc::new(Mutex::new(Vec::<String>::new()));
    let extension_runtime = extensions_support::runtime();
    let extension_cwd = temp.path().display().to_string();
    let extension = {
        let observed = observed.clone();
        let model = model.clone();
        extensions_support::extension(
            &extension_runtime,
            &extension_cwd,
            "<inline:ordered-custom>",
            move |api| {
                api.register_provider(
                    "ordered-custom",
                    ProviderConfigInput {
                        // Frozen model-registry.ts:887–889 requires provider baseUrl,
                        // even when every model already carries its own baseUrl.
                        base_url: Some(model.base_url.clone()),
                        api: Some(model.api.clone()),
                        api_key: Some(JsString::from("private-fixture-key")),
                        models: Some(vec![model.into()]),
                        stream_simple: Some(Arc::new(|model, _context, options| {
                            let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
                            tokio::spawn(async move {
                                let response = pi_ai::types::ProviderResponse {
                                    status: 201.0,
                                    headers: [
                                        ("z-first".to_owned(), "last alphabetically".to_owned()),
                                        ("a-second".to_owned(), "first alphabetically".to_owned()),
                                    ]
                                    .into_iter()
                                    .collect(),
                                };
                                options.unwrap().stream.request.on_response.unwrap()(response, model)
                                    .await
                                    .unwrap();
                                let message = faux_assistant_message("ordered", FauxAssistantMessageOptions::default());
                                sender.push(pi_ai::types::AssistantMessageEvent::Done {
                                    reason: pi_ai::types::DoneReason::Stop,
                                    message: message.clone(),
                                });
                                sender.end(Some(message));
                            });
                            Ok(stream)
                        })),
                        ..Default::default()
                    },
                )
                .unwrap();
                api.on_after_provider_response(move |event: AfterProviderResponseEvent, _context: ExtensionContext| {
                    let observed = observed.clone();
                    Box::pin(async move {
                        observed
                            .lock()
                            .unwrap()
                            .push(serde_json::to_string(&event.headers).unwrap());
                        // The generic event is also the RPC/JSON emission boundary.
                        let wire = serde_json::to_string(
                            &pi_coding_agent::core::extensions::ExtensionEvent::AfterProviderResponse(event),
                        )
                        .unwrap();
                        observed.lock().unwrap().push(wire);
                        Ok(None)
                    })
                        as pi_coding_agent::core::extensions::BoxFuture<
                            'static,
                            Result<Option<()>, pi_coding_agent::core::extensions::HandlerError>,
                        >
                })
                .unwrap();
            },
        )
        .await
    };
    let loader = Arc::new(StaticResourceLoader {
        extensions: Some(LoadExtensionsResult {
            extensions: vec![extension],
            errors: Vec::new(),
            runtime: extension_runtime,
        }),
        ..Default::default()
    });
    let session_dir = temp.path().join("sessions").display().to_string();
    let mut manager = SessionManager::create(
        &JsString::from(&extension_cwd),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .unwrap();
    manager
        .set_metadata_control_db_path(
            Some(&JsString::from(&temp.path().join("control.db").display().to_string())),
            Default::default(),
        )
        .unwrap();
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(extension_cwd.clone())),
        agent_dir: Some(JsString::from(extension_cwd)),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(loader),
        session_manager: Some(manager),
        ..Default::default()
    })
    .await
    .unwrap();
    result.session.prompt("hi", PromptOptions::default()).await.unwrap();
    let observed = observed.lock().unwrap();
    assert!(!observed.is_empty());
    // JSON.stringify({"z-first": ..., "a-second": ...}) in pinned TS preserves this order.
    assert_eq!(
        observed[0],
        r#"{"z-first":"last alphabetically","a-second":"first alphabetically"}"#
    );
    assert!(observed[1].contains(&format!(r#""headers":{}"#, observed[0])));
}

#[path = "support/scripted_registry.rs"]
mod scripted_registry;
