//! Port of `packages/coding-agent/src/core/agent-session-services.ts`.

use crate::utils::paths::node_join;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use pi_js_compat::abort::AbortSignal;

use crate::cli::args::UnknownFlagValue;
use crate::config::get_agent_dir;
use crate::core::extensions::types::{FlagType, FlagValue};
use crate::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
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
    pub model_runtime: Option<ModelRuntime>,
    pub model_runtime_signal: Option<AbortSignal>,
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
    pub model_runtime: ModelRuntime,
    pub settings_manager: Arc<Mutex<SettingsManager>>,
    pub resource_loader: Arc<dyn ResourceLoader>,
    pub diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
}

fn build_resource_loader(
    options: CreateAgentSessionServicesOptions,
    cwd: &JsString,
    agent_dir: &JsString,
    settings_manager: &Arc<Mutex<SettingsManager>>,
) -> Result<Arc<dyn ResourceLoader>, JsString> {
    if let Some(loader) = options.resource_loader {
        return Ok(loader);
    }
    let mut loader_options = options.resource_loader_options.unwrap_or_default();
    loader_options.cwd = cwd.to_owned();
    loader_options.agent_dir = agent_dir.to_owned();
    loader_options.settings_manager = Some(Arc::clone(settings_manager));
    let loader = DefaultResourceLoader::new(loader_options).map_err(JsString::from)?;
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
    model_runtime: &ModelRuntime,
    resource_loader: &Arc<dyn ResourceLoader>,
) -> Vec<AgentSessionRuntimeDiagnostic> {
    let mut diagnostics = Vec::new();
    let extensions_result = resource_loader.get_extensions();
    for registration in extensions_result.runtime.pending_provider_registrations() {
        if let Err(error) = model_runtime.register_provider(&registration.name, registration.config) {
            diagnostics.push(AgentSessionRuntimeDiagnostic {
                diagnostic_type: "error".to_owned(),
                message: (JsString::from("Extension \"") + &registration.extension_path + "\" error: ")
                    + error.message(),
            });
        }
    }
    extensions_result.runtime.clear_pending_provider_registrations();
    diagnostics
}

/// Creates cwd-bound runtime services.
pub async fn create_agent_session_services(
    mut options: CreateAgentSessionServicesOptions,
) -> Result<AgentSessionServices, JsString> {
    let cwd = resolve_path(
        &options.cwd,
        &process_cwd().map_err(|error| error.message().clone())?,
        &PathInputOptions::default(),
    )
    .map_err(|error| error.message().clone())?;
    let agent_dir = options
        .agent_dir
        .as_ref()
        .map(|dir| {
            resolve_path(
                dir,
                &process_cwd().map_err(|error| error.message().clone())?,
                &PathInputOptions::default(),
            )
            .map_err(|error| error.message().clone())
        })
        .transpose()?
        .unwrap_or_else(|| get_agent_dir().display().to_string().into());
    let model_runtime = match options.model_runtime.clone() {
        Some(runtime) => runtime,
        None => ModelRuntime::create(CreateModelRuntimeOptions {
            auth_path: Some(node_join(&agent_dir, "auth.json")),
            models_path: Some(Some(node_join(&agent_dir, "models.json"))),
            signal: options.model_runtime_signal.clone(),
            ..Default::default()
        })
        .await
        .map_err(|error| error.message().clone())?,
    };
    let settings_manager = options.settings_manager.take().unwrap_or_else(|| {
        SettingsManager::create(&cwd, Some(&agent_dir), Default::default()).expect("settings manager must initialize")
    });
    let settings_manager = Arc::new(Mutex::new(settings_manager));
    let extension_flag_values = options.extension_flag_values.take();
    let reload_options = options.resource_loader_reload_options.take();
    let resource_loader = build_resource_loader(options, &cwd, &agent_dir, &settings_manager)?;
    resource_loader
        .reload_with_options(reload_options)
        .await
        .map_err(JsString::from)?;

    let mut diagnostics = register_pending_providers(&model_runtime, &resource_loader);
    diagnostics.extend(apply_extension_flag_values(
        &resource_loader,
        extension_flag_values.as_ref(),
    ));

    Ok(AgentSessionServices {
        cwd,
        agent_dir,
        model_runtime,
        settings_manager,
        resource_loader,
        diagnostics,
    })
}

/// Creates an AgentSession from previously created services.
pub async fn create_agent_session_from_services(
    options: CreateAgentSessionFromServicesOptions,
) -> Result<CreateAgentSessionResult, JsString> {
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
