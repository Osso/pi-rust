//! Behavioral tests for `core/sdk.ts` and `core/agent-session-services.ts`.

use std::sync::Arc;

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
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::CreateAgentSessionOptions;
use pi_coding_agent::core::session_manager::SessionManager;
use serde_json::json;

async fn faux_runtime(faux: &FauxProviderHandle) -> ModelRuntime {
    let mut data = serde_json::Map::new();
    data.insert("faux".to_owned(), json!({"type": "api_key", "key": "faux-key"}));
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(data))),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .expect("runtime");
    runtime.register_native_provider(Arc::clone(&faux.provider));
    runtime
}

fn faux() -> FauxProviderHandle {
    faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "faux-1".to_owned(),
            name: Some("Faux Model".to_owned()),
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
    let session_manager =
        SessionManager::create(&temp.path().display().to_string(), Some(&session_dir), None).expect("session manager");
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(temp.path().display().to_string()),
        agent_dir: Some(temp.path().display().to_string()),
        model_runtime: Some(runtime),
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
    let content = std::fs::read_to_string(file).expect("session file");
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
async fn create_agent_session_keeps_running_without_models() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(serde_json::Map::new()))),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .expect("runtime");
    let session_dir = temp.path().join("sessions").display().to_string();
    let session_manager =
        SessionManager::create(&temp.path().display().to_string(), Some(&session_dir), None).expect("session manager");
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(temp.path().display().to_string()),
        agent_dir: Some(temp.path().display().to_string()),
        model_runtime: Some(runtime),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        ..Default::default()
    })
    .await
    .expect("session without a model");
    assert_eq!(
        result.model_fallback_message.as_deref(),
        Some(pi_coding_agent::core::auth_guidance::format_no_models_available_message().as_str())
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
        "error: {}",
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
        faux_assistant_message("service hello", FauxAssistantMessageOptions::default()).into(),
    ]);
    let services = create_agent_session_services(CreateAgentSessionServicesOptions {
        cwd: temp.path().display().to_string(),
        agent_dir: Some(temp.path().display().to_string()),
        settings_manager: None,
        model_runtime: Some(runtime),
        resource_loader: Some(StaticResourceLoader::new()),
    })
    .await
    .expect("services");
    let session_dir = temp.path().join("sessions").display().to_string();
    let session_manager =
        SessionManager::create(&temp.path().display().to_string(), Some(&session_dir), None).expect("session manager");
    let result = create_agent_session_from_services(CreateAgentSessionFromServicesOptions {
        services,
        session_manager,
        model: Some(model),
        thinking_level: None,
        scoped_models: Vec::new(),
        tools: None,
        exclude_tools: None,
        no_tools: None,
        permission_prompt_tool: None,
        custom_tools: Vec::new(),
        base_tools: Vec::new(),
    })
    .await
    .expect("session");
    result
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");
    assert_eq!(result.session.messages().len(), 2);
}

#[tokio::test]
async fn excluded_tools_are_removed_from_initial_active_names() {
    let temp = tempfile::tempdir().expect("temp dir");
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let session_dir = temp.path().join("sessions").display().to_string();
    let session_manager =
        SessionManager::create(&temp.path().display().to_string(), Some(&session_dir), None).expect("session manager");
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(temp.path().display().to_string()),
        agent_dir: Some(temp.path().display().to_string()),
        model_runtime: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        no_tools: Some(pi_coding_agent::core::sdk::NoTools::All),
        ..Default::default()
    })
    .await
    .expect("session");
    assert!(result.session.get_active_tool_names().is_empty());
}
