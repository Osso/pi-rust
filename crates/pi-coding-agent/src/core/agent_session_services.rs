//! Port of `packages/coding-agent/src/core/agent-session-services.ts`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::config::get_agent_dir;
use crate::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use crate::core::resource_loader::ResourceLoader;
use crate::core::sdk::{CreateAgentSessionOptions, CreateAgentSessionResult, NoTools, create_agent_session};
use crate::core::session_manager::SessionManager;
use crate::core::settings_manager::SettingsManager;
use crate::utils::paths::{PathInputOptions, resolve_path};

/// Non-fatal issues collected while creating services or sessions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionRuntimeDiagnostic {
    #[serde(rename = "type")]
    pub diagnostic_type: String,
    pub message: String,
}

/// Inputs for creating cwd-bound runtime services.
pub struct CreateAgentSessionServicesOptions {
    pub cwd: String,
    pub agent_dir: Option<String>,
    pub settings_manager: Option<SettingsManager>,
    pub model_runtime: Option<ModelRuntime>,
    /// Rust carrier for `DefaultResourceLoader` (owned by `ca-cli`).
    pub resource_loader: Option<Arc<dyn ResourceLoader>>,
}

/// Inputs for creating an AgentSession from already-created services.
pub struct CreateAgentSessionFromServicesOptions {
    pub services: AgentSessionServices,
    pub session_manager: SessionManager,
    pub model: Option<pi_ai::types::Model>,
    pub thinking_level: Option<pi_agent_core::harness::types::ThinkingLevel>,
    pub scoped_models: Vec<crate::core::model_resolver::ScopedModel>,
    pub tools: Option<Vec<String>>,
    pub exclude_tools: Option<Vec<String>>,
    pub no_tools: Option<NoTools>,
    pub permission_prompt_tool: Option<String>,
    pub custom_tools: Vec<crate::core::agent_session::ToolDefinition>,
    pub base_tools: Vec<Arc<dyn pi_agent_core::agent::types::AgentTool>>,
}

/// Coherent cwd-bound runtime services for one effective session cwd.
#[derive(Clone)]
pub struct AgentSessionServices {
    pub cwd: String,
    pub agent_dir: String,
    pub model_runtime: ModelRuntime,
    pub settings_manager: Arc<Mutex<SettingsManager>>,
    pub resource_loader: Arc<dyn ResourceLoader>,
    pub diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
}

/// Creates cwd-bound runtime services.
pub async fn create_agent_session_services(
    options: CreateAgentSessionServicesOptions,
) -> Result<AgentSessionServices, String> {
    let cwd =
        resolve_path(&options.cwd, &process_cwd(), &PathInputOptions::default()).map_err(|error| error.to_string())?;
    let agent_dir = options
        .agent_dir
        .as_ref()
        .map(|dir| resolve_path(dir, &process_cwd(), &PathInputOptions::default()).map_err(|error| error.to_string()))
        .transpose()?
        .unwrap_or_else(|| get_agent_dir().display().to_string());
    let model_runtime = match options.model_runtime {
        Some(runtime) => runtime,
        None => ModelRuntime::create(CreateModelRuntimeOptions {
            auth_path: Some(Path::new(&agent_dir).join("auth.json").display().to_string()),
            models_path: Some(Some(Path::new(&agent_dir).join("models.json").display().to_string())),
            ..Default::default()
        })
        .await
        .map_err(|error| error.to_string())?,
    };
    let settings_manager = options.settings_manager.unwrap_or_else(|| {
        SettingsManager::create(&cwd, Some(Path::new(&agent_dir)), Default::default())
            .expect("settings manager must initialize")
    });
    let settings_manager = Arc::new(Mutex::new(settings_manager));
    let resource_loader = options
        .resource_loader
        .ok_or_else(|| "DefaultResourceLoader is not ported; pass resourceLoader".to_owned())?;
    resource_loader.reload().await?;

    Ok(AgentSessionServices {
        cwd,
        agent_dir,
        model_runtime,
        settings_manager,
        resource_loader,
        diagnostics: Vec::new(),
    })
}

/// Creates an AgentSession from previously created services.
pub async fn create_agent_session_from_services(
    options: CreateAgentSessionFromServicesOptions,
) -> Result<CreateAgentSessionResult, String> {
    let services = options.services;
    create_agent_session(CreateAgentSessionOptions {
        cwd: Some(services.cwd),
        agent_dir: Some(services.agent_dir),
        model_runtime: Some(services.model_runtime),
        settings_manager: Some(services.settings_manager),
        resource_loader: Some(services.resource_loader),
        session_manager: Some(options.session_manager),
        model: options.model,
        thinking_level: options.thinking_level,
        scoped_models: options.scoped_models,
        tools: options.tools,
        exclude_tools: options.exclude_tools,
        no_tools: options.no_tools,
        permission_prompt_tool: options.permission_prompt_tool,
        custom_tools: options.custom_tools,
        base_tools: options.base_tools,
        ..Default::default()
    })
    .await
}

fn process_cwd() -> String {
    std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}
