//! Port of `packages/coding-agent/src/core/agent-session-services.ts`.

use crate::utils::paths::node_join;
use pi_ai::request::CallbackError;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;

use crate::cli::args::UnknownFlagValue;
use crate::config::get_agent_dir;
use crate::core::auth_storage::AuthStorage;
use crate::core::extensions::types::{FlagType, FlagValue};
use crate::core::model_registry::ModelRegistry;
use crate::core::resource_loader::{
    DefaultResourceLoader, DefaultResourceLoaderOptions, ResolveProjectTrust, ResourceLoader,
};
use crate::core::sdk::{CreateAgentSessionOptions, CreateAgentSessionResult, NoTools, create_agent_session};
use crate::core::session_manager::SessionManager;
use crate::core::settings_manager::SettingsManager;
use crate::utils::paths::{PathInputOptions, process_cwd, resolve_path};

/// Non-fatal issues collected while creating services or sessions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionRuntimeDiagnostic {
    #[serde(rename = "type")]
    pub diagnostic_type: String,
    pub message: JsString,
}

/// Inputs for creating cwd-bound runtime services.
pub struct CreateAgentSessionServicesOptions {
    pub cwd: JsString,
    pub agent_dir: Option<JsString>,
    pub settings_manager: Option<SettingsManager>,
    pub model_registry: Option<ModelRegistry>,
    pub auth_storage: Option<AuthStorage>,
    /// CLI values for extension-declared flags, in argument order.
    pub extension_flag_values: Option<IndexMap<String, UnknownFlagValue>>,
    /// `DefaultResourceLoaderOptions` minus `cwd`/`agentDir`/`settingsManager`.
    pub resource_loader_options: Option<DefaultResourceLoaderOptions>,
    /// Rust carrier for an already-built loader (tests and SDK callers).
    pub resource_loader: Option<Arc<dyn ResourceLoader>>,
    /// `ResourceLoaderReloadOptions.resolveProjectTrust`.
    pub resource_loader_reload_options: Option<ResolveProjectTrust>,
}

/// Inputs for creating an AgentSession from already-created services.
pub struct CreateAgentSessionFromServicesOptions {
    pub no_supervisor: Option<bool>,
    pub services: AgentSessionServices,
    pub session_manager: SessionManager,
    pub model: Option<pi_ai::types::Model>,
    pub thinking_level: Option<pi_agent_core::harness::types::ThinkingLevel>,
    pub scoped_models: Vec<crate::core::model_resolver::ScopedModel>,
    pub tools: Option<Vec<String>>,
    pub exclude_tools: Option<Vec<String>>,
    pub no_tools: Option<NoTools>,
    pub permission_prompt_tool: Option<String>,
    pub multi_agent_runtime_role: Option<crate::core::agent_session::MultiAgentRuntimeRole>,
    pub multi_agent_execution_capability: Option<crate::core::agent_session::MultiAgentExecutionCapability>,
    pub custom_tools: Vec<crate::core::agent_session::ToolDefinition>,
    pub base_tools: Vec<Arc<dyn pi_agent_core::agent::types::AgentTool>>,
    pub multi_agent_store: Option<Arc<crate::core::multi_agent_store::MultiAgentStore>>,
    pub on_shared_channel_message_delivered: Option<Arc<dyn Fn(pi_js_compat::js_json::JsString) + Send + Sync>>,
    pub session_start_event: Option<crate::core::extensions::types::SessionStartEvent>,
}

/// Coherent cwd-bound runtime services for one effective session cwd.
#[derive(Clone)]
pub struct AgentSessionServices {
    pub cwd: JsString,
    pub agent_dir: JsString,
    pub model_registry: ModelRegistry,
    pub auth_storage: AuthStorage,
    pub settings_manager: Arc<Mutex<SettingsManager>>,
    pub resource_loader: Arc<dyn ResourceLoader>,
    pub diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
}

fn build_resource_loader(
    options: CreateAgentSessionServicesOptions,
    cwd: &JsString,
    agent_dir: &JsString,
    settings_manager: &Arc<Mutex<SettingsManager>>,
) -> Result<Arc<dyn ResourceLoader>, CallbackError> {
    if let Some(loader) = options.resource_loader {
        return Ok(loader);
    }
    let mut loader_options = options.resource_loader_options.unwrap_or_default();
    loader_options.cwd = cwd.to_owned();
    loader_options.agent_dir = agent_dir.to_owned();
    loader_options.settings_manager = Some(Arc::clone(settings_manager));
    let loader = DefaultResourceLoader::new(loader_options)?;
    Ok(Arc::new(loader))
}

/// `applyExtensionFlagValues`.
fn apply_extension_flag_values(
    resource_loader: &Arc<dyn ResourceLoader>,
    extension_flag_values: Option<&IndexMap<String, UnknownFlagValue>>,
) -> Vec<AgentSessionRuntimeDiagnostic> {
    let Some(extension_flag_values) = extension_flag_values else {
        return Vec::new();
    };
    let mut diagnostics = Vec::new();
    let extensions_result = resource_loader.get_extensions();
    let mut registered_flags: IndexMap<String, FlagType> = IndexMap::new();
    for extension in &extensions_result.extensions {
        for (name, flag) in &extension.read().flags {
            registered_flags.insert(name.clone(), flag.flag_type);
        }
    }

    let mut unknown_flags = Vec::new();
    for (name, value) in extension_flag_values {
        let Some(flag_type) = registered_flags.get(name) else {
            unknown_flags.push(name.clone());
            continue;
        };
        match flag_type {
            FlagType::Boolean => {
                extensions_result.runtime.set_flag_value(name, FlagValue::Boolean(true));
            }
            FlagType::String => match value {
                UnknownFlagValue::String(value) => {
                    extensions_result
                        .runtime
                        .set_flag_value(name, FlagValue::String(value.clone()));
                }
                UnknownFlagValue::Boolean(_) => diagnostics.push(AgentSessionRuntimeDiagnostic {
                    diagnostic_type: "error".to_owned(),
                    message: format!("Extension flag \"--{name}\" requires a value").into(),
                }),
            },
        }
    }

    if !unknown_flags.is_empty() {
        let plural = if unknown_flags.len() == 1 { "" } else { "s" };
        diagnostics.push(AgentSessionRuntimeDiagnostic {
            diagnostic_type: "error".to_owned(),
            message: format!(
                "Unknown option{plural}: {}",
                unknown_flags
                    .iter()
                    .map(|name| format!("--{name}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into(),
        });
    }
    diagnostics
}

fn register_pending_providers(
    model_registry: &ModelRegistry,
    resource_loader: &Arc<dyn ResourceLoader>,
) -> Result<Vec<AgentSessionRuntimeDiagnostic>, CallbackError> {
    let mut diagnostics = Vec::new();
    let extensions_result = resource_loader.get_extensions();
    for registration in extensions_result.runtime.pending_provider_registrations() {
        if let Err(error) = model_registry.register_provider(&registration.name, registration.config) {
            // Frozen agent-session-services.ts:164–169 catches Error.message or
            // String(error); String conversion can itself reject.
            let message = error.error_or_string_message()?;
            diagnostics.push(AgentSessionRuntimeDiagnostic {
                diagnostic_type: "error".to_owned(),
                message: (JsString::from("Extension \"") + &registration.extension_path + "\" error: ") + message,
            });
        }
    }
    extensions_result.runtime.clear_pending_provider_registrations();
    Ok(diagnostics)
}

/// Creates cwd-bound runtime services.
pub async fn create_agent_session_services(
    mut options: CreateAgentSessionServicesOptions,
) -> Result<AgentSessionServices, CallbackError> {
    let cwd = resolve_path(
        &options.cwd,
        &process_cwd().map_err(CallbackError::from)?,
        &PathInputOptions::default(),
    )
    .map_err(CallbackError::from)?;
    let agent_dir = options
        .agent_dir
        .as_ref()
        .filter(|dir| !dir.is_empty())
        .map(|dir| {
            resolve_path(
                dir,
                &process_cwd().map_err(CallbackError::from)?,
                &PathInputOptions::default(),
            )
            .map_err(CallbackError::from)
        })
        .transpose()?
        .map(Ok)
        .unwrap_or_else(get_agent_dir)
        .map_err(CallbackError::from)?;
    let auth_storage = options
        .auth_storage
        .clone()
        .map(Ok)
        .unwrap_or_else(|| AuthStorage::create(Some(&node_join(&agent_dir, "auth.json"))))
        .map_err(CallbackError::from)?;
    let settings_manager = match options.settings_manager.take() {
        Some(settings) => settings,
        None => SettingsManager::create(&cwd, Some(&agent_dir), Default::default())?,
    };
    let model_registry =
        options.model_registry.clone().map(Ok).unwrap_or_else(|| {
            ModelRegistry::create(auth_storage.clone(), Some(node_join(&agent_dir, "models.json")))
        })?;
    let settings_manager = Arc::new(Mutex::new(settings_manager));
    let extension_flag_values = options.extension_flag_values.take();
    let reload_options = options.resource_loader_reload_options.take();
    let resource_loader = build_resource_loader(options, &cwd, &agent_dir, &settings_manager)?;
    resource_loader.reload_with_options(reload_options).await?;

    let mut diagnostics = register_pending_providers(&model_registry, &resource_loader)?;
    diagnostics.extend(apply_extension_flag_values(
        &resource_loader,
        extension_flag_values.as_ref(),
    ));

    Ok(AgentSessionServices {
        cwd,
        agent_dir,
        model_registry,
        auth_storage,
        settings_manager,
        resource_loader,
        diagnostics,
    })
}

/// Creates an AgentSession from previously created services.
pub async fn create_agent_session_from_services(
    options: CreateAgentSessionFromServicesOptions,
) -> Result<CreateAgentSessionResult, CallbackError> {
    let services = options.services;
    create_agent_session(CreateAgentSessionOptions {
        no_supervisor: options.no_supervisor,
        cwd: Some(services.cwd),
        agent_dir: Some(services.agent_dir),
        model_registry: Some(services.model_registry),
        auth_storage: Some(services.auth_storage),
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
        multi_agent_runtime_role: options.multi_agent_runtime_role,
        multi_agent_execution_capability: options.multi_agent_execution_capability,
        custom_tools: options.custom_tools,
        base_tools: options.base_tools,
        multi_agent_store: options.multi_agent_store,
        on_shared_channel_message_delivered: options.on_shared_channel_message_delivered,
        session_start_event: options.session_start_event,
        ..Default::default()
    })
    .await
}
