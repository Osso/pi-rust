//! Minimal `AgentSession` fixture for interactive component tests.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_agent_core::agent::{Agent, AgentInitialState, AgentOptions};
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::providers::faux::{FauxModelDefinition, FauxProviderHandle, RegisterFauxProviderOptions, faux_provider};
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{Context, Model};
use pi_coding_agent::core::agent_session::{AgentSession, AgentSessionConfig};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};

pub struct SessionFixture {
    pub session: AgentSession,
    /// Kept alive so the native faux provider stays registered.
    #[allow(dead_code)]
    pub faux: FauxProviderHandle,
}

fn faux_model() -> FauxModelDefinition {
    FauxModelDefinition {
        id: "faux-1".to_owned(),
        name: Some("Faux Model".into()),
        reasoning: Some(true),
        input: None,
        cost: None,
        context_window: Some(100_000.0),
        max_tokens: Some(4096.0),
    }
}

pub async fn fixture(cwd: &std::path::Path, thinking_level: ThinkingLevel) -> SessionFixture {
    let mut auth_data = serde_json::Map::new();
    auth_data.insert(
        "faux".to_owned(),
        serde_json::json!({"type": "api_key", "key": "faux-key"}),
    );
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(auth_data)).unwrap();
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![faux_model()]),
        ..Default::default()
    });
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    let model = faux.get_model(None).expect("faux model");
    let stream_fn: pi_agent_core::agent::types::StreamFn =
        Arc::new(move |model: Model, context: Context, options: SimpleStreamOptions| {
            Box::pin(async move {
                pi_ai::compat::stream_simple(model, context, options).map_err(pi_agent_core::AgentError::from)
            })
        });
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentInitialState {
            model: Some(model),
            system_prompt: Some(JsString::default()),
            thinking_level: Some(thinking_level),
            tools: Some(Vec::new()),
            messages: None,
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    })
    .expect("agent");
    let cwd = cwd.display().to_string();
    let session_manager = SessionManager::in_memory(Some(&JsString::from(&cwd)), None).expect("in-memory session");
    let settings_manager = SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default());
    let session = AgentSession::new(AgentSessionConfig {
        agent,
        session_manager: Arc::new(std::sync::Mutex::new(session_manager)),
        settings_manager: Arc::new(std::sync::Mutex::new(settings_manager)),
        cwd: JsString::from(cwd.clone()),
        scoped_models: Vec::new(),
        resource_loader: pi_coding_agent::core::resource_loader::StaticResourceLoader::new(),
        model_registry: runtime,
        initial_active_tool_names: None,
        allowed_tool_names: None,
        excluded_tool_names: None,
        permission_prompt_tool: None,
        agent_dir: Some(JsString::from(cwd)),
        control_db_path: None,
        base_tools: Vec::new(),
        custom_tool_definitions: Vec::new(),
        custom_tools: Vec::new(),
        multi_agent_runtime_role: None,
        multi_agent_execution_capability: None,
        thinking_phase_timeout_ms: None,
        no_supervisor: None,
        supervisor_decision_requester: None,
        multi_agent_store: None,
        multi_agent_agent_id: None,
        multi_agent_parent_session_id: None,
        multi_agent_requires_agent_id: false,
        disable_runtime_coordination_inbound: false,
        on_shared_channel_message_delivered: None,
        extension_runner_ref: None,
        session_start_event: None,
    })
    .expect("session");
    SessionFixture { session, faux }
}

#[path = "scripted_registry.rs"]
mod scripted_registry;
