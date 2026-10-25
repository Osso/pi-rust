//! Port of `packages/coding-agent/src/main.ts`.
//!
//! This module mirrors the startup helpers of the TypeScript entry point.
//! The orchestration (`main`) is added once the in-flight mode/runtime slices
//! land on `master`; see `docs/porting/traceability/main.md` for the current
//! mapping and the explicit gaps.

use pi_ai::request::CallbackError;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::io::IsTerminal;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use pi_ai::models::models_are_equal;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

use crate::cli::args::{
    Args, ExtensionFlag, ExtensionFlagDefault, ExtensionFlagType, ListModels, Mode, parse_args, print_help,
};
use crate::cli::config_selector::TuiConfigSelector;
use crate::cli::file_processor::{ProcessFileOptions, process_file_arguments};
use crate::cli::initial_message::{InitialMessageInput, InitialMessageResult, build_initial_message};
use crate::cli::list_extensions::list_extensions;
use crate::cli::list_models::{StdioListModelsIo, list_models};
use crate::cli::list_tools::list_tools;
use crate::cli::project_trust::{CreateProjectTrustContextOptions, create_project_trust_context};
use crate::cli::resident_console_command::parse_resident_console_args;
use crate::cli::{
    ControlCommandDependencies, DebugCommandDependencies, LoginCommandDependencies, SessionsCommandDependencies,
    handle_control_command, handle_debug_command, handle_login_command, handle_sessions_command,
};
use crate::config::{InstallContext, get_agent_dir, get_package_dir, get_user_state_root, package_config};
use crate::core::agent_session::{
    ExtensionBindings, MultiAgentRuntimeRole, bind_agent_session_mutation_target_resolver,
    create_multi_agent_execution_capability,
};
use crate::core::agent_session_runtime::{
    CreateAgentSessionRuntimeFactory, CreateAgentSessionRuntimeOptions, CreateAgentSessionRuntimeResult,
    create_agent_session_runtime,
};
use crate::core::agent_session_services::{
    AgentSessionRuntimeDiagnostic, AgentSessionServices, CreateAgentSessionFromServicesOptions,
    CreateAgentSessionServicesOptions, create_agent_session_from_services, create_agent_session_services,
};
use crate::core::auth_guidance::format_no_models_available_message;
use crate::core::export_html::{ExportOptions, export_from_file};
use crate::core::extensions::loader::{ExtensionFactory, ExtensionRegistry};
use crate::core::external_session_importer::{
    ExternalSessionImportOptions, import_external_session_alias, is_external_session_alias,
};
use crate::core::http_dispatcher::configure_http_dispatcher;
use crate::core::lifecycle_coordinator::LifecycleCoordinator;
use crate::core::model_catalog_cache::{
    EnsureModelCatalogOptions, ModelCatalogRefreshResult, ensure_model_catalog_fresh,
    load_openrouter_catalog_at_startup,
};
use crate::core::model_registry::ModelRegistry;
use crate::core::model_resolver::{ResolveCliModelOptions, ScopedModel, resolve_cli_model, resolve_model_scope};
use crate::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use crate::core::output_guard::{restore_stdout, take_over_stdout};
use crate::core::project_trust::{AppMode, ResolveProjectTrustedOptions, resolve_project_trusted};
use crate::core::resource_loader::{DefaultResourceLoaderOptions, InlineExtension, ResourceLoader};
use crate::core::sdk::{CreateAgentSessionOptions, NoTools};
use crate::core::self_restart::{
    SelfRestartWaitDependencies, append_self_restart_notice, apply_self_restart_request, consume_self_restart_request,
    resolve_restart_interrupted_tool_calls, wait_for_self_restart_parent_exit,
};
use crate::core::session_control_db::{
    claim_latest_incoming_message, get_control_db_path, prepare_control_db_for_self_restart,
};
use crate::core::session_cwd::{MissingSessionCwdError, get_missing_session_cwd_issue, reopen_session_with_cwd};
use crate::core::session_directory::{SessionDirectoryOptions, reconcile_session_runtime_bindings};
use crate::core::session_manager::SessionManagerError;
use crate::core::session_manager::{
    NewSessionOptions, SessionInfo, SessionManager, assert_valid_session_id, get_default_session_dir,
};
use crate::core::settings_manager::{DefaultProjectTrust, SettingsManager, SettingsManagerCreateOptions};
use crate::core::timings::{TimingLabel, print_timings, reset_timings, time};
use crate::core::trust_manager::{ProjectTrustDecision, ProjectTrustStore, has_trust_requiring_project_resources};
use crate::migrations::{run_migrations, show_deprecation_warnings};
use crate::modes::interactive::interactive_mode::{
    CancelMultiAgent, InteractiveMode, InteractiveModeOptions, SteerMultiAgent,
};
use crate::modes::interactive::theme::{init_theme, stop_theme_watcher};
use crate::modes::print_mode::{PrintMode, PrintModeOptions, run_print_mode};
use crate::modes::rpc::rpc_mode::run_rpc_mode;
use crate::package_manager_cli::{
    CommandOutcome, DefaultPackageCommandEffects, DefaultProjectTrustExtensionsLoader, execute_config_command,
    execute_package_command,
};
use crate::utils::chalk::Chalk;
use crate::utils::git_project::read_git_common_directory;
use crate::utils::git_worktree::resolve_worktree;
use crate::utils::iso_time::now_iso_string;
use crate::utils::paths::{PathError, PathInputOptions, is_local_path, normalize_path, process_cwd, resolve_path};
use crate::utils::windows_self_update::cleanup_windows_self_update_quarantine;

/// `process.exit(code)` represented as an error so helpers can stop the flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MainExit(pub i32);

/// Native glue keeps source `process.exit` separate from a thrown error.
#[derive(Debug, Clone, PartialEq)]
pub enum MainFailure {
    Exit(MainExit),
    Thrown(CallbackError),
}

impl From<MainExit> for MainFailure {
    fn from(exit: MainExit) -> Self {
        Self::Exit(exit)
    }
}

impl From<CallbackError> for MainFailure {
    fn from(error: CallbackError) -> Self {
        Self::Thrown(error)
    }
}

/// Result of a helper that may call `process.exit` or throw.
pub type MainResult<T> = Result<T, MainFailure>;

/// `MainOptions`.
#[derive(Default)]
pub struct MainOptions {
    /// Inline extension factories appended after the first-party factories.
    pub extension_factories: Vec<InlineExtension>,
}

/// Every `<first-party:...>` factory registered by `main.ts`, in source order.
pub const FIRST_PARTY_EXTENSION_NAMES: &[&str] = &[
    "approval-controls",
    "ask-secret",
    "browser-cli",
    "claude-bash-hook",
    "claude-memory-enrich",
    "claude-memory-session-end",
    "codex-fast",
    "codex-image-generation",
    "codex-usage",
    "codex-web-search",
    "debug",
    "docs-tree-context",
    "agents-core",
    "agent-viewer",
    "agents-mailbox",
    "default-footer",
    "effort",
    "goal",
    "bwrap",
    "pyrun",
    "loop",
    "openai-remote-compact",
    "run-plan",
    "safe",
    "spec-validation",
    "session-archive",
    "session-autoname",
    "self-restart",
    "session-id",
];

/// `firstPartyExtensionFactory`.
pub fn first_party_extension_factory(name: &str, factory: ExtensionFactory) -> InlineExtension {
    InlineExtension::Factory {
        extension_path: Some(format!("<first-party:{name}>").into()),
        factory,
    }
}

/// `createFirstPartyExtensionFactories`. The shared store, runtime handles,
/// fast-mode authority, and HTTP client are created by `main`.
///
/// The factories are registered under their source ids and then emitted in
/// `main.ts` order, so loading order matches the TypeScript list.
#[derive(Clone)]
pub struct FirstPartyOrchestration {
    pub store: Arc<MultiAgentStore>,
    pub runtime_handles: crate::bundled_extensions::agents_core::runtime::MultiAgentRuntimeHandles,
    pub select_agent_view: Arc<Mutex<Option<crate::bundled_extensions::agents_core::runtime::SelectAgentView>>>,
}

pub fn create_first_party_extension_factories(
    orchestration: Option<FirstPartyOrchestration>,
    fetch: Arc<dyn pi_ai::request::Fetch>,
    agent_dir: JsString,
    runtime_extension_factories: Arc<dyn Fn() -> Vec<InlineExtension> + Send + Sync>,
) -> Vec<InlineExtension> {
    use crate::bundled_extensions::agents_core::session_factories::{
        AgentSessionChildSession, ChildExtensionFactories, CreateChildSession, CreateChildSessionManager,
        ProductionAttachedSessionFactoryOptions, ProductionChildAgentSessionFactoryOptions, UnboundChildAgentSession,
        create_production_attached_session_factory, create_production_child_agent_session_factory,
    };

    let agent_options = orchestration.as_ref().map(|orchestration| {
        let store = &orchestration.store;
        let create_session: CreateChildSession = Arc::new(|options: CreateAgentSessionOptions| {
            Box::pin(async move {
                let result = crate::core::sdk::create_agent_session(options)
                    .await
                    .map_err(|error| error.message)?;
                Ok(Arc::new(AgentSessionChildSession::new(result.session)) as Arc<dyn UnboundChildAgentSession>)
            })
        });
        let create_session_manager: CreateChildSessionManager = Arc::new(|cwd, session_dir, options| {
            SessionManager::create(cwd, session_dir, Some(options)).map_err(|error| error.message().clone())
        });
        let child_agent_session_factory =
            create_production_child_agent_session_factory(ProductionChildAgentSessionFactoryOptions {
                agent_dir: Some(agent_dir.clone()),
                create_session: Arc::clone(&create_session),
                create_session_manager,
                extension_factories: Some(ChildExtensionFactories::Dynamic(Arc::clone(
                    &runtime_extension_factories,
                ))),
                multi_agent_store: Some(Arc::clone(&store)),
                session_dir: None,
            });
        let attached_session_factory =
            create_production_attached_session_factory(ProductionAttachedSessionFactoryOptions {
                agent_dir: Some(agent_dir),
                create_session,
                extension_factories: Some(ChildExtensionFactories::Dynamic(runtime_extension_factories)),
                multi_agent_store: Some(Arc::clone(&store)),
                session_dir: None,
            });
        let select_agent_view = {
            let slot = Arc::clone(&orchestration.select_agent_view);
            Arc::new(move |agent_id: &JsString| {
                let selector = slot.lock().expect("agent view selector").clone();
                match selector {
                    Some(selector) => selector(agent_id),
                    None => Box::pin(async { None }),
                }
            }) as crate::bundled_extensions::agents_core::runtime::SelectAgentView
        };
        crate::bundled_extensions::agents_core::runtime::MultiAgentExtensionOptions {
            create_attached_session: Some(attached_session_factory),
            create_child_session: Some(child_agent_session_factory),
            runtime_handles: Some(orchestration.runtime_handles.clone()),
            select_agent_view: Some(select_agent_view),
            store: Some(Arc::clone(store)),
            ..Default::default()
        }
    });
    let mut registry = ExtensionRegistry::new();
    crate::bundled_extensions::register_small_extensions(
        &mut registry,
        &crate::bundled_extensions::SmallExtensionsOptions {
            get_debug_repl: debug_repl_gap_provider(),
            fast_mode_authority: crate::bundled_extensions::codex_fast::FastModeAuthority::default(),
            fetch: Some(fetch),
            multi_agent_store: orchestration
                .as_ref()
                .map(|orchestration| Arc::clone(&orchestration.store)),
        },
    );
    if let Some(agent_options) = &agent_options {
        crate::bundled_extensions::register_agents_extensions(&mut registry, agent_options);
    }
    crate::bundled_extensions::register_bundled_extensions(&mut registry);
    let pyrun_factory = match agent_options {
        Some(options) => pyrun_extension_factory(options),
        None => crate::bundled_extensions::pyrun::index::pyrun_extension_factory(Default::default()),
    };
    registry.register("<first-party:pyrun>", pyrun_factory);

    FIRST_PARTY_EXTENSION_NAMES
        .iter()
        .filter(|name| {
            orchestration.is_some()
                || !["agents-core", "agent-viewer", "agents-mailbox", "agents-status", "goal"].contains(name)
        })
        .filter_map(|name| {
            registry
                .get(&JsString::from(format!("<first-party:{name}>")))
                .map(|factory| first_party_extension_factory(name, Arc::clone(factory)))
        })
        .collect()
}

/// `core/debug-repl.ts` is an approved gap: the provider reports the gap
/// instead of returning a fake REPL.
pub fn debug_repl_gap_provider() -> crate::bundled_extensions::debug::DebugReplControllerProvider {
    Arc::new(|| Arc::new(DebugReplGapController))
}

struct DebugReplGapController;

impl crate::bundled_extensions::debug::DebugReplController for DebugReplGapController {
    fn enable(
        &self,
        _session_id: &pi_js_compat::js_json::JsString,
    ) -> crate::core::extensions::types::BoxFuture<
        'static,
        Result<pi_js_compat::js_json::JsString, crate::core::extensions::types::HandlerError>,
    > {
        Box::pin(async { Err(crate::core::extensions::types::HandlerError::new(DEBUG_REPL_GAP)) })
    }

    fn disable(
        &self,
    ) -> crate::core::extensions::types::BoxFuture<'static, Result<(), crate::core::extensions::types::HandlerError>>
    {
        Box::pin(async { Err(crate::core::extensions::types::HandlerError::new(DEBUG_REPL_GAP)) })
    }
}

pub const DEBUG_REPL_GAP: &str = "Debug REPL is not ported (core/debug-repl.ts)";

/// `pyrunExtension(pi, { piRequestHandlers: [multiAgentPiRequestHandler] })`.
fn pyrun_extension_factory(
    agent_options: crate::bundled_extensions::agents_core::runtime::MultiAgentExtensionOptions,
) -> ExtensionFactory {
    let store = agent_options.store.clone();
    Arc::new(move |pi| {
        let mut options = crate::bundled_extensions::pyrun::PyrunExtensionOptions {
            background_jobs_store: store.clone(),
            ..Default::default()
        };
        let handler = Arc::new(
            crate::bundled_extensions::agents_core::runtime::create_multi_agent_pi_request_handler(
                &agent_options,
                Some(crate::bundled_extensions::agents_core::parent_agent_journal::JournalWriter::from_api(pi)),
            ),
        );
        let dispatcher: crate::bundled_extensions::pyrun::eval_tool::PyrunPiRequestDispatcher =
            Arc::new(move |request, context, signal, active_tool_call_id| {
                let handler = Arc::clone(&handler);
                Box::pin(async move {
                    if !handler.handles_method(&request.method) {
                        return Ok(None);
                    }
                    let value = pi_js_compat::js_json!({
                        "method": request.method,
                        "params": request.params,
                    });
                    handler
                        .handle(&value, &context, signal, active_tool_call_id)
                        .await
                        .map(Some)
                })
            });
        options.pi_request_handlers.push(dispatcher);
        let result = crate::bundled_extensions::pyrun::install_pyrun_extension(pi, options);
        Box::pin(async move { result })
    })
}

/// `new MultiAgentStore()` as created at module load in the source.
pub fn create_first_party_multi_agent_store() -> Arc<MultiAgentStore> {
    Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()))
}

// =============================================================================
// Runtime factory (`createRuntime` in the source)
// =============================================================================

/// Captured state for the `createRuntime` closure in `main.ts`.
#[derive(Clone)]
pub struct RuntimeFactoryOptions {
    pub parsed: Args,
    pub agent_dir: JsString,
    pub startup_default_project_trust: DefaultProjectTrust,
    pub trust_store: Arc<ProjectTrustStore>,
    pub trust_prompt_mode: AppMode,
    pub app_mode: AppMode,
    pub project_trust_by_cwd: Arc<Mutex<HashMap<JsString, bool>>>,
    pub additional_extension_paths: Vec<JsString>,
    pub additional_skill_paths: Vec<JsString>,
    pub additional_prompt_template_paths: Vec<JsString>,
    pub additional_theme_paths: Vec<JsString>,
    pub extension_factories: Vec<InlineExtension>,
    pub orchestration: Option<FirstPartyOrchestration>,
}

struct InteractiveOrchestrationControls {
    wake_wait_agents_after_steering: Option<Arc<dyn Fn() + Send + Sync>>,
    steer_multi_agent: SteerMultiAgent,
    cancel_multi_agent: Option<CancelMultiAgent>,
}

fn create_interactive_orchestration_controls(
    orchestration: Option<&FirstPartyOrchestration>,
    control_db_path: JsString,
) -> InteractiveOrchestrationControls {
    let Some(orchestration) = orchestration else {
        return InteractiveOrchestrationControls {
            wake_wait_agents_after_steering: None,
            steer_multi_agent: Arc::new(|_, _, _| Box::pin(async { Err("Agent steering is unavailable".into()) })),
            cancel_multi_agent: None,
        };
    };
    let wake_handles = orchestration.runtime_handles.clone();
    let store = Arc::clone(&orchestration.store);
    let handles = orchestration.runtime_handles.clone();
    let steer_multi_agent: SteerMultiAgent = Arc::new(move |agent_id, message, session_id| {
        let store = Arc::clone(&store);
        let handles = handles.clone();
        let control_db_path = control_db_path.clone();
        Box::pin(async move {
            if control_db_path.is_empty() {
                return Err("Agent steering is unavailable".into());
            }
            crate::bundled_extensions::agents_core::control_runtime::request_interactive_agent_steering(
                &store,
                &handles,
                &agent_id,
                &message,
                &crate::bundled_extensions::agents_core::control_runtime::AgentSteeringRuntimeBinding {
                    actor_agent_id: None,
                    control_db_path,
                    session_id,
                },
            )
        })
    });
    let store = Arc::clone(&orchestration.store);
    let handles = orchestration.runtime_handles.clone();
    let cancel_multi_agent: CancelMultiAgent = Arc::new(move |agent_id| {
        let store = Arc::clone(&store);
        let handles = handles.clone();
        Box::pin(async move {
            crate::bundled_extensions::agents_core::control_runtime::cancel_owned_agent_runtime(
                &store,
                &handles,
                &agent_id,
                Some(&"cancelled interactively".into()),
            )
            .await
            .map(|_| ())
            .map_err(|error| error.message)
        })
    });
    InteractiveOrchestrationControls {
        wake_wait_agents_after_steering: Some(Arc::new(move || {
            crate::bundled_extensions::agents_core::control_runtime::wake_wait_agents_after_steering(&wake_handles)
        })),
        steer_multi_agent,
        cancel_multi_agent: Some(cancel_multi_agent),
    }
}

fn trust_decision_is_trusted(store: &ProjectTrustStore, cwd: &JsString) -> Result<bool, JsString> {
    store
        .get(cwd)
        .map(|decision| decision == ProjectTrustDecision::Trusted)
        .map_err(|error| error.message().clone())
}

/// Runtime reason strings mapped to `session_start` metadata.
fn session_start_event(
    reason: &str,
    previous_session_file: Option<JsString>,
) -> crate::core::extensions::types::SessionStartEvent {
    use crate::core::extensions::types::{SessionStartEvent, SessionStartReason};
    let reason = match reason {
        "reload" => SessionStartReason::Reload,
        "new" => SessionStartReason::New,
        "resume" => SessionStartReason::Resume,
        "fork" => SessionStartReason::Fork,
        "restart" => SessionStartReason::Restart,
        _ => SessionStartReason::Startup,
    };
    SessionStartEvent {
        reason,
        previous_session_file,
    }
}

fn project_trust_context(
    cwd: &JsString,
    mode: AppMode,
    has_ui: bool,
) -> crate::core::extensions::types::ProjectTrustContext {
    create_project_trust_context(CreateProjectTrustContextOptions {
        cwd: cwd.clone(),
        mode,
        has_ui,
        startup_ui: None,
    })
}

async fn create_runtime(
    options: &RuntimeFactoryOptions,
    request: CreateAgentSessionRuntimeOptions,
) -> Result<CreateAgentSessionRuntimeResult, CallbackError> {
    let mut session_manager = request.session_manager;
    let parsed = &options.parsed;
    session_manager
        .set_metadata_control_db_path(
            Some(&JsString::from(get_control_db_path(&get_user_state_root()?))),
            crate::core::session_manager::SetMetadataControlDbPathOptions {
                index_message_text: None,
                cwd_override: None,
            },
        )
        .map_err(CallbackError::from)?;
    if let Some(orchestration) = &options.orchestration {
        orchestration
            .store
            .restore_from_session_manager(&session_manager)
            .map_err(CallbackError::from)?;
    }

    let is_initial_runtime = request.session_start_reason == "startup";
    let project_trust_diagnostics = Arc::new(Mutex::new(Vec::new()));
    let cwd = request.cwd.clone();
    let cached_project_trust = options
        .project_trust_by_cwd
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&cwd)
        .copied();
    let has_trust_requiring_resources = has_trust_requiring_project_resources(&cwd).map_err(CallbackError::from)?;
    let should_resolve_project_trust =
        parsed.project_trust_override.is_none() && cached_project_trust.is_none() && has_trust_requiring_resources;
    let project_trusted = match (
        should_resolve_project_trust,
        cached_project_trust.or(parsed.project_trust_override),
    ) {
        (true, _) => false,
        (false, Some(trusted)) => trusted,
        (false, None) => !has_trust_requiring_resources || trust_decision_is_trusted(&options.trust_store, &cwd)?,
    };

    let runtime_settings_manager = SettingsManager::create(
        &cwd,
        Some(&options.agent_dir),
        SettingsManagerCreateOptions {
            project_trusted: Some(project_trusted),
        },
    )
    .map_err(CallbackError::from)?;

    let reload_options = if should_resolve_project_trust {
        let trust_store = Arc::clone(&options.trust_store);
        let project_trust_by_cwd = Arc::clone(&options.project_trust_by_cwd);
        let diagnostics = Arc::clone(&project_trust_diagnostics);
        let trust_override = parsed.project_trust_override;
        let default_project_trust = options.startup_default_project_trust;
        let trust_prompt_mode = options.trust_prompt_mode;
        let app_mode = options.app_mode;
        let resolver_cwd = cwd.clone();
        let provided_trust_context = request.project_trust_context;
        Some(Arc::new(
            move |extensions_result: crate::core::extensions::types::LoadExtensionsResult| {
                let trust_store = Arc::clone(&trust_store);
                let project_trust_by_cwd = Arc::clone(&project_trust_by_cwd);
                let diagnostics = Arc::clone(&diagnostics);
                let cwd = resolver_cwd.clone();
                let provided_trust_context = provided_trust_context.clone();
                Box::pin(async move {
                    let mode = if is_initial_runtime {
                        trust_prompt_mode
                    } else {
                        app_mode
                    };
                    let context = provided_trust_context.unwrap_or_else(|| {
                        project_trust_context(
                            &cwd,
                            mode,
                            is_initial_runtime && trust_prompt_mode == AppMode::Interactive,
                        )
                    });
                    let on_extension_error = |message: JsString| {
                        diagnostics
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .push(AgentSessionRuntimeDiagnostic {
                                diagnostic_type: "warning".to_owned(),
                                message,
                            });
                    };
                    let trusted = resolve_project_trusted(ResolveProjectTrustedOptions {
                        cwd: &cwd,
                        trust_store: &trust_store,
                        trust_override,
                        default_project_trust: Some(default_project_trust),
                        extensions_result: Some(&extensions_result),
                        project_trust_context: &context,
                        on_extension_error: Some(&on_extension_error),
                    })
                    .await
                    .map_err(|error| error.message().clone())?;
                    project_trust_by_cwd
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .insert(cwd, trusted);
                    Ok(trusted)
                }) as pi_ai::request::BoxFuture<'static, Result<bool, JsString>>
            },
        ) as crate::core::resource_loader::ResolveProjectTrust)
    } else {
        None
    };

    let resource_loader_options = DefaultResourceLoaderOptions {
        additional_extension_paths: options.additional_extension_paths.clone(),
        additional_skill_paths: options.additional_skill_paths.clone(),
        additional_prompt_template_paths: options.additional_prompt_template_paths.clone(),
        additional_theme_paths: options.additional_theme_paths.clone(),
        extension_factories: options.extension_factories.clone(),
        no_extensions: parsed.no_extensions.is_some_and(|value| value),
        no_skills: parsed.no_skills.is_some_and(|value| value),
        no_prompt_templates: parsed.no_prompt_templates.is_some_and(|value| value),
        no_themes: parsed.no_themes.is_some_and(|value| value),
        no_context_files: parsed.no_context_files.is_some_and(|value| value),
        system_prompt: parsed.system_prompt.as_deref().map(JsString::from),
        append_system_prompt: parsed
            .append_system_prompt
            .as_ref()
            .map(|prompts| prompts.iter().map(|prompt| JsString::from(prompt.as_str())).collect()),
        ..Default::default()
    };

    let services = create_agent_session_services(CreateAgentSessionServicesOptions {
        cwd: cwd.clone(),
        agent_dir: Some(options.agent_dir.clone()),
        settings_manager: Some(runtime_settings_manager),
        model_registry: None,
        auth_storage: None,
        extension_flag_values: Some(parsed.unknown_flags.clone()),
        resource_loader_options: Some(resource_loader_options),
        resource_loader: None,
        resource_loader_reload_options: reload_options,
    })
    .await?;

    let settings_manager = Arc::clone(&services.settings_manager);
    let resource_loader = Arc::clone(&services.resource_loader);
    let model_registry = services.model_registry.clone();
    let mut diagnostics = project_trust_diagnostics
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    diagnostics.extend(services.diagnostics.clone());
    diagnostics.extend(collect_settings_diagnostics(
        &mut settings_manager.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        "runtime creation",
    ));
    diagnostics.extend(resource_loader.get_extensions().errors.into_iter().map(|error| {
        AgentSessionRuntimeDiagnostic {
            diagnostic_type: "error".to_owned(),
            message: js_format!("Failed to load extension \"{}\": {}", error.path, error.error),
        }
    }));

    let model_patterns = parsed
        .models
        .as_ref()
        .map(|models| models.iter().map(|model| JsString::from(model.as_str())).collect())
        .or_else(|| {
            settings_manager
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get_enabled_models()
        });
    let scoped_models = match model_patterns.filter(|patterns| !patterns.is_empty()) {
        Some(patterns) => resolve_model_scope(&patterns, &model_registry)
            .await
            .map_err(|error| error.message)?,
        None => Vec::new(),
    };

    let has_existing_session = !session_manager.build_session_context().messages.is_empty();
    let (session_options, cli_thinking_from_model, option_diagnostics) = {
        let settings_guard = settings_manager.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let built = build_session_options(
            parsed,
            &scoped_models,
            has_existing_session,
            &model_registry,
            &settings_guard,
        )
        .map_err(|error| error.message)?;
        (built.options, built.cli_thinking_from_model, built.diagnostics)
    };
    diagnostics.extend(option_diagnostics);

    if let Some(api_key) = parsed.api_key.as_deref() {
        match session_options.model.as_ref() {
            None => diagnostics.push(AgentSessionRuntimeDiagnostic {
                diagnostic_type: "error".to_owned(),
                message: "--api-key requires a model to be specified via --model, --provider/--model, or --models"
                    .into(),
            }),
            Some(model) => {
                model_registry
                    .auth_storage()
                    .set_runtime_api_key(&model.provider, &JsString::from(api_key));
            }
        }
    }

    let result = create_agent_session_from_services(CreateAgentSessionFromServicesOptions {
        no_supervisor: options.parsed.no_supervisor,
        services: AgentSessionServices {
            cwd: services.cwd.clone(),
            agent_dir: services.agent_dir.clone(),
            model_registry: services.model_registry.clone(),
            auth_storage: services.auth_storage.clone(),
            settings_manager: Arc::clone(&services.settings_manager),
            resource_loader: Arc::clone(&services.resource_loader),
            diagnostics: services.diagnostics.clone(),
        },
        session_manager,
        model: session_options.model,
        thinking_level: session_options.thinking_level,
        scoped_models: session_options.scoped_models,
        tools: session_options.tools,
        exclude_tools: session_options.exclude_tools,
        no_tools: session_options.no_tools,
        permission_prompt_tool: session_options.permission_prompt_tool,
        multi_agent_runtime_role: (parsed.no_supervisor != Some(true)).then_some(MultiAgentRuntimeRole::Orchestrator),
        multi_agent_execution_capability: (parsed.no_supervisor != Some(true))
            .then(create_multi_agent_execution_capability),
        custom_tools: Vec::new(),
        base_tools: Vec::new(),
        multi_agent_store: options
            .orchestration
            .as_ref()
            .map(|orchestration| Arc::clone(&orchestration.store)),
        on_shared_channel_message_delivered: options.orchestration.as_ref().map(|orchestration| {
            let handles = orchestration.runtime_handles.clone();
            Arc::new(move |prompt: JsString| {
                crate::bundled_extensions::agents_core::runtime::wake_wait_agents_after_coordination(&handles, &prompt);
            }) as Arc<dyn Fn(JsString) + Send + Sync>
        }),
        session_start_event: Some(session_start_event(
            &request.session_start_reason,
            request.previous_session_file.clone(),
        )),
    })
    .await?;

    let orchestration = options.orchestration.clone();
    bind_agent_session_mutation_target_resolver(
        &result.session,
        Arc::new(move || match &orchestration {
            Some(orchestration) => {
                crate::bundled_extensions::agents_core::runtime::resolve_selected_session_mutation_target(
                    &orchestration.store,
                    &orchestration.runtime_handles,
                )
            }
            None => Ok(None),
        }),
    );

    let cli_thinking_override = parsed.thinking.is_some() || cli_thinking_from_model;
    if cli_thinking_override {
        result
            .session
            .set_thinking_level(result.session.thinking_level())
            .map_err(|error| pi_js_compat::js_json::encode_node_utf8(error.message()))?;
    }

    Ok(CreateAgentSessionRuntimeResult {
        session: result.session,
        services,
        diagnostics,
        model_fallback_message: result.model_fallback_message,
    })
}

/// Builds the `createRuntime` factory captured by `main`.
pub fn create_runtime_factory(options: RuntimeFactoryOptions) -> CreateAgentSessionRuntimeFactory {
    Arc::new(move |request| {
        let options = options.clone();
        Box::pin(async move { create_runtime(&options, request).await })
    })
}

// =============================================================================
// Output
// =============================================================================

pub use pi_coding_agent_base::console::{console_error, console_log, write_stderr, write_stdout};

// =============================================================================
// Startup predicates
// =============================================================================

/// `isTruthyEnvFlag`.
pub fn is_truthy_env_flag(value: Option<&str>) -> bool {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return false;
    };
    value == "1" || value.to_lowercase() == "true" || value.to_lowercase() == "yes"
}

/// `resolveAppMode`.
pub fn resolve_app_mode(parsed: &Args, stdin_is_tty: bool, stdout_is_tty: bool) -> AppMode {
    match parsed.mode {
        Some(Mode::Rpc) => AppMode::Rpc,
        Some(Mode::Json) => AppMode::Json,
        _ => {
            if parsed.print.is_some_and(|print| print) || !stdin_is_tty || !stdout_is_tty {
                AppMode::Print
            } else {
                AppMode::Interactive
            }
        }
    }
}

/// `toPrintOutputMode`.
pub const fn to_print_output_mode(app_mode: AppMode) -> Mode {
    match app_mode {
        AppMode::Json => Mode::Json,
        _ => Mode::Text,
    }
}

/// `isPlainRuntimeMetadataCommand`.
pub fn is_plain_runtime_metadata_command(parsed: &Args) -> bool {
    !parsed.print.is_some_and(|print| print)
        && parsed.mode.is_none()
        && (parsed.help == Some(true)
            || parsed.list_models.is_some()
            || parsed.refresh_models == Some(true)
            || parsed.list_tools == Some(true)
            || parsed.list_extensions == Some(true))
}

struct OfflineCatalogFetch;
impl pi_ai::request::Fetch for OfflineCatalogFetch {
    fn fetch(
        &self,
        _request: pi_ai::request::HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, CallbackError>> {
        Box::pin(async { Err(CallbackError::new("Network disabled by offline mode")) })
    }
}

pub async fn refresh_openrouter_catalog(
    offline_mode: bool,
    force: bool,
    fetch: Arc<dyn pi_ai::request::Fetch>,
) -> Result<ModelCatalogRefreshResult, pi_js_compat::node_error::NativeNodeError> {
    let fetch = if offline_mode {
        Arc::new(OfflineCatalogFetch) as Arc<dyn pi_ai::request::Fetch>
    } else {
        fetch
    };
    ensure_model_catalog_fresh(EnsureModelCatalogOptions {
        force,
        fetch,
        now: Arc::new(pi_js_compat::date::js_now_millis),
    })
    .await
}

pub fn format_model_catalog_refresh_summary(result: &ModelCatalogRefreshResult) -> JsString {
    let path = pi_js_compat::node_fs::decode_node_path(result.cache_path.as_os_str());
    js_format!(
        "OpenRouter models: fetched {}, cached {}, bundled {}, merged {}; cache {}",
        result.fetched_count,
        result.cached_count,
        result.bundled_count,
        result.models.len(),
        path
    )
}

/// `isTruthyEnvFlag(process.env.PI_OFFLINE)`.
pub fn offline_mode_from_env(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--offline")
        || is_truthy_env_flag(pi_js_compat::process_env::get("PI_OFFLINE").as_deref())
}

// =============================================================================
// Diagnostics
// =============================================================================

/// `collectSettingsDiagnostics`.
pub fn collect_settings_diagnostics(
    settings_manager: &mut SettingsManager,
    context: &str,
) -> Vec<AgentSessionRuntimeDiagnostic> {
    settings_manager
        .drain_errors()
        .into_iter()
        .map(|error| AgentSessionRuntimeDiagnostic {
            diagnostic_type: "warning".to_owned(),
            message: js_format!("({}, {} settings) {}", context, error.scope.as_str(), error.message),
        })
        .collect()
}

/// `reportDiagnostics`.
pub fn report_diagnostics(diagnostics: &[AgentSessionRuntimeDiagnostic]) {
    let chalk = Chalk::stderr();
    for diagnostic in diagnostics {
        let message = match diagnostic.diagnostic_type.as_str() {
            "error" => chalk.red(js_format!("Error: {}", diagnostic.message)),
            "warning" => chalk.yellow(js_format!("Warning: {}", diagnostic.message)),
            _ => chalk.dim(diagnostic.message.clone()),
        };
        console_error(&message);
    }
}

// =============================================================================
// Abort helpers
// =============================================================================

/// `AbortSignal.timeout(ms)` as a cancellation token. The timer is aborted when
/// the returned guard is dropped.
pub struct TimeoutSignal {
    signal: AbortSignal,
    timer: tokio::task::JoinHandle<()>,
}

impl TimeoutSignal {
    pub fn new(timeout_ms: u64) -> Self {
        let controller = pi_js_compat::abort::AbortController::new();
        let signal = controller.signal();
        let timer_signal = controller;
        let timer = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)).await;
            timer_signal.abort(None);
        });
        Self { signal, timer }
    }

    pub fn token(&self) -> AbortSignal {
        self.signal.clone()
    }
}

impl Drop for TimeoutSignal {
    fn drop(&mut self) {
        self.timer.abort();
    }
}

// =============================================================================
// Initial message
// =============================================================================

/// `prepareInitialMessage`.
pub async fn prepare_initial_message(
    parsed: &mut Args,
    auto_resize_images: bool,
    stdin_content: Option<&str>,
) -> Result<InitialMessageResult, JsString> {
    if parsed.file_args.is_empty() {
        return Ok(build_initial_message(InitialMessageInput {
            parsed,
            file_text: None,
            file_images: None,
            stdin_content,
        }));
    }

    let processed = process_file_arguments(
        &parsed.file_args,
        Some(ProcessFileOptions {
            auto_resize_images: Some(auto_resize_images),
        }),
        &process_cwd().map_err(|error| error.message().clone())?,
    )
    .await
    .map_err(|error| error.message().clone())?;
    Ok(build_initial_message(InitialMessageInput {
        parsed,
        file_text: Some(&processed.text),
        file_images: Some(&processed.images),
        stdin_content,
    }))
}

// =============================================================================
// Session resolution
// =============================================================================

/// `ResolvedSession`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedSession {
    Path(JsString),
    Local(JsString),
    Global { path: JsString, cwd: JsString },
    NotFound(JsString),
}

fn resolve_from_cwd(input: &JsString) -> Result<JsString, PathError> {
    resolve_path(input, process_cwd()?, &PathInputOptions::default())
}

/// `findLocalSessionByExactId`.
pub fn find_local_session_by_exact_id(
    session_id: &JsString,
    cwd: &JsString,
    session_dir: Option<&JsString>,
    control_db_path: Option<&JsString>,
) -> Result<Option<ResolvedSession>, JsString> {
    let local_sessions =
        SessionManager::list(cwd, session_dir, None, control_db_path).map_err(|error| error.message().clone())?;
    Ok(local_sessions
        .into_iter()
        .find(|session| &session.id == session_id)
        .map(|session| ResolvedSession::Local(session.path)))
}

/// `sessionBelongsToProject`.
pub fn session_belongs_to_project(session_cwd: &JsString, cwd: &JsString) -> bool {
    if let (Ok(session), Ok(current)) = (resolve_from_cwd(session_cwd), resolve_from_cwd(cwd))
        && session == current
    {
        return true;
    }

    let session_git_directory = read_git_common_directory(session_cwd);
    let current_git_directory = read_git_common_directory(cwd);
    session_git_directory.is_some() && session_git_directory == current_git_directory
}

fn find_session_match<'a>(sessions: &'a [SessionInfo], session_arg: &JsString) -> Option<&'a SessionInfo> {
    sessions
        .iter()
        .find(|session| &session.id == session_arg)
        .or_else(|| sessions.iter().find(|session| session.id.starts_with(session_arg)))
}

/// `resolveSessionPath`.
pub fn resolve_session_path(
    session_arg: &JsString,
    cwd: &JsString,
    session_dir: Option<&JsString>,
    control_db_path: Option<&JsString>,
) -> Result<ResolvedSession, JsString> {
    let path_like = session_arg.contains("/") || session_arg.contains("\\") || session_arg.ends_with(".jsonl");
    let resolved_session_arg_path = if path_like {
        Some(resolve_path(session_arg, cwd, &PathInputOptions::default()).map_err(|error| error.message().clone())?)
    } else {
        None
    };
    if let Some(resolved) = &resolved_session_arg_path
        && (session_arg.ends_with(".jsonl") || Path::new(&encode_node_utf8(resolved)).exists())
    {
        return Ok(ResolvedSession::Path(resolved.clone()));
    }

    if is_external_session_alias(session_arg) {
        let imported = import_external_session_alias(
            session_arg,
            &ExternalSessionImportOptions {
                home_dir: None,
                session_dir: session_dir.cloned(),
                fallback_cwd: Some(cwd.clone()),
            },
        )
        .map_err(|error| error.message().clone())?;
        return Ok(match imported {
            Some(session) => ResolvedSession::Path(session.path),
            None => ResolvedSession::NotFound(session_arg.clone()),
        });
    }

    if let Some(resolved) = resolved_session_arg_path {
        return Ok(ResolvedSession::Path(resolved));
    }

    let local_sessions =
        SessionManager::list(cwd, session_dir, None, control_db_path).map_err(|error| error.message().clone())?;
    if let Some(local_match) = find_session_match(&local_sessions, session_arg) {
        return Ok(ResolvedSession::Local(local_match.path.clone()));
    }

    let all_sessions =
        SessionManager::list_all(session_dir, None, control_db_path).map_err(|error| error.message().clone())?;
    if let Some(global_match) = find_session_match(&all_sessions, session_arg) {
        if session_belongs_to_project(&global_match.cwd, cwd) {
            return Ok(ResolvedSession::Local(global_match.path.clone()));
        }
        return Ok(ResolvedSession::Global {
            path: global_match.path.clone(),
            cwd: global_match.cwd.clone(),
        });
    }

    Ok(ResolvedSession::NotFound(session_arg.clone()))
}

/// `promptConfirm`.
pub async fn prompt_confirm(message: &str) -> bool {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    let mut stdout = tokio::io::stdout();
    let _ = stdout.write_all(format!("{message} [y/N] ").as_bytes()).await;
    let _ = stdout.flush().await;

    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin);
    let mut answer = String::new();
    if reader.read_line(&mut answer).await.is_err() {
        return false;
    }
    let answer = answer.to_lowercase();
    answer == "y" || answer == "yes"
}

// =============================================================================
// Validation
// =============================================================================

/// `validateForkFlags` error text (`None` when the flags do not conflict).
pub fn fork_flag_conflict(parsed: &Args) -> Option<String> {
    parsed.fork.as_ref()?;
    let conflicting_flags = [
        parsed.session.as_ref().map(|_| "--session"),
        parsed
            .continue_session
            .is_some_and(|value| value)
            .then_some("--continue"),
        parsed.resume.is_some_and(|value| value).then_some("--resume"),
        parsed.no_session.is_some_and(|value| value).then_some("--no-session"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    (!conflicting_flags.is_empty())
        .then(|| format!("Error: --fork cannot be combined with {}", conflicting_flags.join(", ")))
}

/// `validateSessionIdFlags` conflict text (`None` when the flags do not conflict).
pub fn session_id_flag_conflict(parsed: &Args) -> Option<String> {
    parsed.session_id.as_ref()?;
    let conflicting_flags = [
        parsed.session.as_ref().map(|_| "--session"),
        parsed
            .continue_session
            .is_some_and(|value| value)
            .then_some("--continue"),
        parsed.resume.is_some_and(|value| value).then_some("--resume"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    (!conflicting_flags.is_empty()).then(|| {
        format!(
            "Error: --session-id cannot be combined with {}",
            conflicting_flags.join(", ")
        )
    })
}

/// `validateForkFlags`.
pub fn validate_fork_flags(parsed: &Args) -> Result<(), MainExit> {
    let Some(message) = fork_flag_conflict(parsed) else {
        return Ok(());
    };
    console_error(&Chalk::stderr().red(&message));
    Err(MainExit(1))
}

/// `validateSessionIdFlags`.
pub fn validate_session_id_flags(parsed: &Args) -> Result<(), MainExit> {
    let Some(session_id) = parsed.session_id.as_deref() else {
        return Ok(());
    };
    if let Some(message) = session_id_flag_conflict(parsed) {
        console_error(&Chalk::stderr().red(&message));
        return Err(MainExit(1));
    }
    if let Err(error) = assert_valid_session_id(&session_id.into()) {
        console_error(Chalk::stderr().red(js_format!("Error: {}", error.message())));
        return Err(MainExit(1));
    }
    Ok(())
}

// =============================================================================
// Error reporting wrappers
// =============================================================================

/// `runCliActionOrReportError`: returns the resulting exit code.
pub async fn run_cli_action_or_report_error<F, Fut>(action: F) -> i32
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(), JsString>>,
{
    match action().await {
        Ok(()) => 0,
        Err(message) => {
            console_error(Chalk::stderr().red(js_format!("Error: {}", message)));
            1
        }
    }
}

/// Outcome of [`run_interactive_action_or_report_terminal_error`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InteractiveActionResult {
    pub ok: bool,
    /// `process.exitCode` set by the helper.
    pub exit_code: Option<i32>,
}

/// `runInteractiveActionOrReportTerminalError`.
pub async fn run_interactive_action_or_report_terminal_error<F, Fut>(
    action: F,
    on_terminal_error: Option<&dyn Fn()>,
) -> Result<InteractiveActionResult, CallbackError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(), CallbackError>>,
{
    match action().await {
        Ok(()) => Ok(InteractiveActionResult {
            ok: true,
            exit_code: None,
        }),
        Err(error) => {
            crate::modes::interactive::theme::stop_theme_watcher();
            let raw_mode_failure = error
                .original
                .as_deref()
                .is_some_and(|original| pi_tui::is_terminal_raw_mode_failure(original))
                || error
                    .message
                    .to_lowercase()
                    .starts_with("setrawmode failed with errno:");
            if raw_mode_failure {
                console_error(Chalk::stderr().red(js_format!("Error: {}", error.message)));
                if let Some(on_terminal_error) = on_terminal_error {
                    on_terminal_error();
                }
                return Ok(InteractiveActionResult {
                    ok: false,
                    exit_code: Some(1),
                });
            }
            Err(error)
        }
    }
}

fn report_session_error(error: &SessionManagerError) -> MainFailure {
    console_error(Chalk::stderr().red(js_format!("Error: {}", error.message())));
    MainFailure::Exit(MainExit(1))
}

fn open_session_or_exit(path: &JsString, session_dir: Option<&JsString>) -> MainResult<SessionManager> {
    SessionManager::open(path, session_dir, None).map_err(|error| report_session_error(&error))
}

fn fork_session_or_exit(
    source_path: &JsString,
    cwd: &JsString,
    session_dir: Option<&JsString>,
    session_id: Option<&JsString>,
) -> MainResult<SessionManager> {
    SessionManager::fork_from(
        source_path,
        cwd,
        session_dir,
        session_id.map(|id| NewSessionOptions {
            id: Some(id.clone()),
            ..Default::default()
        }),
        None,
    )
    .map_err(|error| report_session_error(&error))
}

/// Session loader used by the `--resume` picker.
struct MainSessionsLoader {
    cwd: JsString,
    session_dir: Option<JsString>,
    control_db_path: Option<JsString>,
    scope: MainSessionsScope,
}

#[derive(Clone, Copy)]
enum MainSessionsScope {
    Current,
    All,
    Archived,
}

impl crate::cli::session_picker::SessionLoader for MainSessionsLoader {
    fn load<'a>(
        &'a self,
        on_progress: Option<crate::cli::session_picker::SessionListProgress<'a>>,
    ) -> crate::cli::session_picker::SessionListFuture<'a, Result<Vec<SessionInfo>, JsString>> {
        Box::pin(async move {
            let session_dir = self.session_dir.as_ref();
            let control_db_path = self.control_db_path.as_ref();
            match self.scope {
                MainSessionsScope::Current => {
                    SessionManager::list(&self.cwd, session_dir, on_progress, control_db_path)
                        .map_err(|error| error.message().clone())
                }
                MainSessionsScope::All => SessionManager::list_all(session_dir, on_progress, control_db_path)
                    .map_err(|error| error.message().clone()),
                MainSessionsScope::Archived => {
                    SessionManager::list_archived(session_dir, control_db_path).map_err(|error| error.message().clone())
                }
            }
        })
    }
}

/// `selectSession(...)` with the process terminal and environment.
async fn select_session_for_process(
    cwd: &JsString,
    session_dir: Option<&JsString>,
    settings_manager: &SettingsManager,
    control_db_path: Option<&JsString>,
) -> Result<crate::cli::session_picker::SelectSessionOutcome, CallbackError> {
    let loader = |scope| {
        Arc::new(MainSessionsLoader {
            cwd: cwd.clone(),
            session_dir: session_dir.cloned(),
            control_db_path: control_db_path.cloned(),
            scope,
        }) as Arc<dyn crate::cli::session_picker::SessionLoader>
    };
    let validate_selection = control_db_path.map(|control_db_path| {
        let control_db_path = control_db_path.clone();
        let session_dir = session_dir.cloned();
        Arc::new(move |path: &JsString| {
            let selected =
                SessionManager::open(path, session_dir.as_ref(), None).map_err(|error| error.message().clone())?;
            crate::core::session_control_db::assert_main_session_runtime_available(
                &control_db_path,
                selected.get_session_id(),
            )
            .map_err(|error| error.message().clone())
        }) as Arc<dyn Fn(&JsString) -> Result<(), JsString>>
    });
    let env: BTreeMap<String, String> = pi_js_compat::process_env::vars().into_iter().collect();
    crate::cli::session_picker::select_session(
        loader(MainSessionsScope::Current),
        loader(MainSessionsScope::All),
        crate::cli::session_picker::SelectSessionOptions {
            settings_manager,
            terminal: Rc::new(RefCell::new(pi_tui::terminal::ProcessTerminal::new())),
            env: &env,
            control_db_path,
            archived_sessions_loader: Some(loader(MainSessionsScope::Archived)),
            validate_selection,
        },
    )
    .await
}

// =============================================================================
// Session manager creation
// =============================================================================

/// `createSessionManager`.
pub async fn create_session_manager(
    parsed: &Args,
    cwd: &JsString,
    session_dir: Option<&JsString>,
    settings_manager: &SettingsManager,
    control_db_path: Option<&JsString>,
) -> MainResult<SessionManager> {
    let session_id = parsed.session_id.as_deref().map(JsString::from);
    if parsed.no_session.is_some_and(|value| value)
        || parsed.help.is_some_and(|value| value)
        || parsed.list_models.is_some()
        || parsed.list_tools.is_some_and(|value| value)
        || parsed.list_extensions.is_some_and(|value| value)
    {
        let options = session_id.clone().map(|id| NewSessionOptions {
            id: Some(id),
            ..Default::default()
        });
        return SessionManager::in_memory(Some(cwd), options).map_err(|error| report_session_error(&error));
    }

    if let Some(fork) = parsed.fork.as_deref().map(JsString::from) {
        if let Some(session_id) = &session_id
            && let Ok(Some(ResolvedSession::Local(_))) =
                find_local_session_by_exact_id(session_id, cwd, session_dir, control_db_path)
        {
            console_error(Chalk::stderr().red(js_format!("Session already exists with id '{}'", session_id)));
            return Err(MainExit(1).into());
        }

        let resolved = resolve_session_path(&fork, cwd, session_dir, control_db_path).map_err(|error| {
            console_error(Chalk::stderr().red(js_format!("Error: {}", error)));
            MainExit(1)
        })?;

        return match resolved {
            ResolvedSession::Path(path) | ResolvedSession::Local(path) | ResolvedSession::Global { path, .. } => {
                fork_session_or_exit(&path, cwd, session_dir, session_id.as_ref())
            }
            ResolvedSession::NotFound(arg) => {
                console_error(Chalk::stderr().red(js_format!("No session found matching '{}'", arg)));
                Err(MainExit(1).into())
            }
        };
    }

    if let Some(session) = parsed.session.clone() {
        let resolved = resolve_session_path(&session, cwd, session_dir, control_db_path).map_err(|error| {
            console_error(Chalk::stderr().red(js_format!("Error: {}", error)));
            MainExit(1)
        })?;

        return match resolved {
            ResolvedSession::Path(path) | ResolvedSession::Local(path) => open_session_or_exit(&path, session_dir),
            ResolvedSession::Global { path, cwd: global_cwd } => {
                console_log(Chalk::stdout().yellow(js_format!("Session found in different project: {}", global_cwd)));
                let should_fork = prompt_confirm("Fork this session into current directory?").await;
                if !should_fork {
                    console_log(Chalk::stdout().dim("Aborted."));
                    return Err(MainExit(0).into());
                }
                fork_session_or_exit(&path, cwd, session_dir, None)
            }
            ResolvedSession::NotFound(arg) => {
                console_error(Chalk::stderr().red(js_format!("No session found matching '{}'", arg)));
                Err(MainExit(1).into())
            }
        };
    }

    if parsed.resume.is_some_and(|value| value) {
        let outcome = select_session_for_process(cwd, session_dir, settings_manager, control_db_path).await?;
        stop_theme_watcher();
        match outcome {
            crate::cli::session_picker::SelectSessionOutcome::Selected(path) => {
                return open_session_or_exit(&path, session_dir);
            }
            crate::cli::session_picker::SelectSessionOutcome::Cancelled => {
                console_log(Chalk::stdout().dim("No session selected"));
                return Err(MainExit(0).into());
            }
            crate::cli::session_picker::SelectSessionOutcome::Exited => return Err(MainExit(0).into()),
        }
    }

    if parsed.continue_session.is_some_and(|value| value) {
        return SessionManager::continue_recent(cwd, session_dir).map_err(|error| report_session_error(&error));
    }

    if let Some(session_id) = &session_id
        && let Ok(Some(ResolvedSession::Local(path))) =
            find_local_session_by_exact_id(session_id, cwd, session_dir, control_db_path)
    {
        return open_session_or_exit(&path, session_dir);
    }
    if let Some(session_id) = &session_id {
        console_error(Chalk::stderr().yellow(js_format!(
            "Warning: No project session found with id '{}'; creating a new session with that id.",
            session_id
        )));
    }

    let options = session_id.map(|id| NewSessionOptions {
        id: Some(id),
        ..Default::default()
    });
    SessionManager::create(cwd, session_dir, options).map_err(|error| report_session_error(&error))
}

// =============================================================================
// Session options
// =============================================================================

/// `buildSessionOptions` result.
pub struct SessionOptionsBuild {
    pub options: CreateAgentSessionOptions,
    pub cli_thinking_from_model: bool,
    pub diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
}

/// `buildSessionOptions`.
pub fn build_session_options(
    parsed: &Args,
    scoped_models: &[ScopedModel],
    has_existing_session: bool,
    model_registry: &ModelRegistry,
    settings_manager: &SettingsManager,
) -> Result<SessionOptionsBuild, pi_js_compat::node_error::NativeNodeError> {
    let mut options = CreateAgentSessionOptions {
        no_supervisor: parsed.no_supervisor,
        ..Default::default()
    };
    let mut diagnostics = Vec::new();
    let mut cli_thinking_from_model = false;

    if let Some(cli_model) = parsed.model.as_deref() {
        let resolved = resolve_cli_model(
            &ResolveCliModelOptions {
                cli_provider: parsed.provider.clone(),
                cli_model: Some(cli_model.into()),
                cli_thinking: parsed.thinking,
            },
            model_registry,
        )?;
        if let Some(warning) = resolved.warning {
            diagnostics.push(AgentSessionRuntimeDiagnostic {
                diagnostic_type: "warning".to_owned(),
                message: warning.into(),
            });
        }
        if let Some(error) = resolved.error {
            diagnostics.push(AgentSessionRuntimeDiagnostic {
                diagnostic_type: "error".to_owned(),
                message: error.into(),
            });
        }
        if let Some(model) = resolved.model {
            options.model = Some(model);
            if parsed.thinking.is_none()
                && let Some(thinking_level) = resolved.thinking_level
            {
                options.thinking_level = Some(thinking_level);
                cli_thinking_from_model = true;
            }
        }
    }

    if options.model.is_none() && !scoped_models.is_empty() && !has_existing_session {
        let saved_provider = settings_manager.get_default_provider();
        let saved_model_id = settings_manager.get_default_model();
        let saved_model = match (saved_provider, saved_model_id) {
            (Some(provider), Some(model_id)) => model_registry.find(provider, model_id),
            _ => None,
        };
        let saved_in_scope = saved_model.as_ref().and_then(|saved| {
            scoped_models
                .iter()
                .find(|scoped| models_are_equal(Some(&scoped.model), Some(saved)))
        });
        if let Some(scoped) = saved_in_scope {
            options.model = Some(scoped.model.clone());
            if parsed.thinking.is_none()
                && let Some(thinking_level) = scoped.thinking_level
            {
                options.thinking_level = Some(thinking_level);
            }
        } else {
            options.model = Some(scoped_models[0].model.clone());
            if parsed.thinking.is_none()
                && let Some(thinking_level) = scoped_models[0].thinking_level
            {
                options.thinking_level = Some(thinking_level);
            }
        }
    }

    if let Some(thinking) = parsed.thinking {
        options.thinking_level = Some(thinking);
    }

    if !scoped_models.is_empty() {
        options.scoped_models = scoped_models
            .iter()
            .map(|scoped| ScopedModel {
                model: scoped.model.clone(),
                thinking_level: scoped.thinking_level,
            })
            .collect();
    }

    if parsed.no_tools.is_some_and(|value| value) {
        options.no_tools = Some(NoTools::All);
    } else if parsed.no_builtin_tools.is_some_and(|value| value) {
        options.no_tools = Some(NoTools::Builtin);
    }
    if let Some(tools) = &parsed.tools {
        options.tools = Some(tools.clone());
    }
    if let Some(exclude_tools) = &parsed.exclude_tools {
        options.exclude_tools = Some(exclude_tools.clone());
    }
    if let Some(permission_prompt_tool) = &parsed.permission_prompt_tool {
        options.permission_prompt_tool = Some(permission_prompt_tool.clone());
    }

    Ok(SessionOptionsBuild {
        options,
        cli_thinking_from_model,
        diagnostics,
    })
}

/// `resolveCliPaths`.
pub fn resolve_cli_paths(cwd: &JsString, paths: Option<&[String]>) -> Result<Option<Vec<JsString>>, JsString> {
    paths
        .map(|paths| {
            paths
                .iter()
                .map(|value| {
                    if is_local_path(value) {
                        resolve_path(value, cwd, &PathInputOptions::default()).map_err(|error| error.message().clone())
                    } else {
                        Ok(value.into())
                    }
                })
                .collect()
        })
        .transpose()
}

/// `normalizePath` for the `--session-dir` argument.
pub fn normalize_session_dir(value: &str) -> Result<JsString, JsString> {
    normalize_path(value, &PathInputOptions::default()).map_err(|error| error.message().clone())
}

/// Default session directory for a cwd (`getDefaultSessionDir`).
pub fn default_session_dir(cwd: &JsString, agent_dir: Option<&JsString>) -> Result<JsString, CallbackError> {
    get_default_session_dir(cwd, agent_dir).map_err(Into::into)
}

/// `ListModels` search text (`None` means all models).
pub fn list_models_search(parsed: &Args) -> Option<Option<&str>> {
    match parsed.list_models.as_ref()? {
        ListModels::All => Some(None),
        ListModels::Search(pattern) => Some(Some(pattern.as_str())),
    }
}

// =============================================================================
// CLI orchestration
// =============================================================================

struct CliControlDependencies;
impl ControlCommandDependencies for CliControlDependencies {}

struct CliSessionsDependencies;
impl SessionsCommandDependencies for CliSessionsDependencies {}

struct CliDebugDependencies {
    agent_dir: JsString,
}

impl DebugCommandDependencies for CliDebugDependencies {
    fn agent_dir(&self) -> Result<JsString, CallbackError> {
        Ok(self.agent_dir.clone())
    }
}

fn print_command_outcome(outcome: &CommandOutcome) {
    for line in &outcome.stdout {
        console_log(line);
    }
    for line in &outcome.stderr {
        console_error(line);
    }
}

fn extension_flags_from_loader(resource_loader: &Arc<dyn ResourceLoader>) -> Vec<ExtensionFlag> {
    resource_loader
        .get_extensions()
        .extensions
        .iter()
        .flat_map(|extension| extension.read().flags.values().cloned().collect::<Vec<_>>())
        .map(|flag| ExtensionFlag {
            name: flag.name.clone(),
            description: flag.description.clone(),
            kind: match flag.flag_type {
                crate::core::extensions::types::FlagType::Boolean => ExtensionFlagType::Boolean,
                crate::core::extensions::types::FlagType::String => ExtensionFlagType::String,
            },
            default: flag.default.as_ref().map(|value| match value {
                crate::core::extensions::types::FlagValue::Boolean(value) => ExtensionFlagDefault::Boolean(*value),
                crate::core::extensions::types::FlagValue::String(value) => ExtensionFlagDefault::String(value.clone()),
            }),
            extension_path: flag.extension_path.clone(),
        })
        .collect()
}

/// `readPipedStdin`: `setEncoding("utf8")` decodes with replacement, so the text is well-formed.
/// A stream error is uncaught in TS and terminates the process; it propagates here.
async fn read_piped_stdin() -> Result<Option<String>, JsString> {
    if std::io::stdin().is_terminal() {
        return Ok(None);
    }
    use tokio::io::AsyncReadExt;
    let mut data = Vec::new();
    tokio::io::stdin()
        .read_to_end(&mut data)
        .await
        .map_err(|error| JsString::from(error.to_string()))?;
    let data = String::from_utf8_lossy(&data);
    let trimmed = pi_js_compat::string::js_trim(&data);
    Ok((!trimmed.is_empty()).then(|| trimmed.to_owned()))
}

async fn prompt_for_missing_session_cwd(
    issue: &crate::core::session_cwd::SessionCwdIssue,
    settings_manager: &SettingsManager,
) -> Result<Option<JsString>, CallbackError> {
    let env: BTreeMap<String, String> = pi_js_compat::process_env::vars().into_iter().collect();
    let terminal = Rc::new(RefCell::new(pi_tui::terminal::ProcessTerminal::new()));
    let options = vec![
        (JsString::from("Continue"), Some(issue.fallback_cwd.clone())),
        (JsString::from("Cancel"), None),
    ];
    crate::cli::startup_ui::show_startup_selector(
        settings_manager,
        terminal,
        &env,
        &crate::core::session_cwd::format_missing_session_cwd_prompt(issue),
        options,
    )
    .await
    .map(|selected| selected.flatten())
}

fn parent_process_id() -> u32 {
    #[cfg(unix)]
    {
        std::os::unix::process::parent_id()
    }
    #[cfg(not(unix))]
    {
        0
    }
}

/// Runs `main.ts` and returns the process exit code.
///
/// `Err` carries a thrown error; the CLI entry point prints it as
/// `Error: <message>` and exits 1.
pub async fn main(args: &[String], options: MainOptions) -> Result<i32, CallbackError> {
    match Box::pin(run_main_inner(args, options)).await {
        Ok(Some(code)) => Ok(code),
        Ok(None) => Ok(0),
        Err(message) => Err(message),
    }
}

/// Rust entry glue: `runCliActionOrReportError(() => main(...))` with the
/// explicit exit code preserved.
pub async fn run_main_or_report_error(args: &[String], options: MainOptions) -> i32 {
    match main(args, options).await {
        Ok(code) => code,
        Err(message) => {
            console_error(Chalk::stderr().red(js_format!("Error: {}", message.message)));
            1
        }
    }
}

fn reject_invalid_standalone_worker_startup(args: &[String]) -> bool {
    let parsed = parse_args(args);
    if parsed.no_supervisor != Some(true) {
        return false;
    }
    let requests_console = args.iter().any(|arg| arg == "--supervisor" || arg == "--architect");
    let has_errors = parsed
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.kind == crate::cli::args::DiagnosticType::Error);
    if !has_errors && !requests_console {
        return false;
    }
    let diagnostics: Vec<_> = parsed
        .diagnostics
        .into_iter()
        .map(|diagnostic| AgentSessionRuntimeDiagnostic {
            diagnostic_type: diagnostic.kind.as_str().into(),
            message: diagnostic.message.into(),
        })
        .collect();
    report_diagnostics(&diagnostics);
    if requests_console {
        console_error("Error: resident consoles are unavailable in standalone worker mode");
    }
    true
}

async fn run_main_inner(args: &[String], options: MainOptions) -> Result<Option<i32>, CallbackError> {
    if args == ["relay", "serve"] {
        crate::core::host_relay::run_relay_serve().await?;
        return Ok(None);
    }
    if args == ["relay", "connect"] {
        crate::core::host_relay_resident::run_relay_connect().await?;
        return Ok(None);
    }
    if reject_invalid_standalone_worker_startup(args) {
        return Ok(Some(1));
    }
    match parse_resident_console_args(args) {
        Ok(Some(command)) => {
            crate::cli::resident_console_command::run_resident_console_command(command).await?;
            return Ok(None);
        }
        Ok(None) => {}
        Err(message) => return Err(message.into()),
    }
    if args.len() == 1 && args[0] == "architect" {
        load_openrouter_catalog_at_startup()
            .await
            .map_err(CallbackError::from)?;
        crate::architect::run_architect_service().await?;
        return Ok(None);
    }
    if args.len() == 1 && args[0] == "supervisor" {
        load_openrouter_catalog_at_startup()
            .await
            .map_err(CallbackError::from)?;
        crate::supervisor::run_supervisor_service().await?;
        return Ok(None);
    }

    reset_timings(TimingLabel::Main);
    let package_command_extension_factories = options.extension_factories.clone();
    let offline_mode = offline_mode_from_env(args);
    if offline_mode {
        pi_js_compat::process_env::set("PI_OFFLINE", "1");
        pi_js_compat::process_env::set("PI_SKIP_VERSION_CHECK", "1");
    }

    if cfg!(windows) {
        cleanup_windows_self_update_quarantine(std::path::Path::new(&pi_js_compat::js_json::encode_node_utf8(
            &get_package_dir()?,
        )));
    }

    let mut cwd = process_cwd().map_err(CallbackError::from)?;
    let agent_dir = get_agent_dir()?;
    let _bootstrap_settings_manager = SettingsManager::create(
        &cwd,
        Some(&agent_dir),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    )
    .map_err(CallbackError::from)?;
    let fetch = configure_http_dispatcher(crate::core::http_dispatcher::DEFAULT_HTTP_IDLE_TIMEOUT_MS as f64)?;

    let package_effects = DefaultPackageCommandEffects {
        cwd: cwd.clone(),
        agent_dir: agent_dir.clone(),
        install_context: InstallContext::for_process()?,
        entrypoint: std::env::current_exe()
            .ok()
            .map(|path| JsString::from(path.display().to_string())),
        extensions_loader: Arc::new(DefaultProjectTrustExtensionsLoader::default()),
        config_selector: Rc::new(TuiConfigSelector {
            terminal: Rc::new(RefCell::new(pi_tui::terminal::ProcessTerminal::new())),
        }),
        fetch: Arc::clone(&fetch),
        extension_factories: package_command_extension_factories.clone(),
    };

    let command_args: Vec<JsString> = args.iter().map(|arg| JsString::from(arg.as_str())).collect();
    if let Some(outcome) = execute_package_command(&command_args, &package_effects).await? {
        print_command_outcome(&outcome);
        let code = outcome.exit_code.unwrap_or(0);
        return Ok(Some(code));
    }
    if let Some(outcome) = execute_config_command(&command_args, &package_effects).await? {
        print_command_outcome(&outcome);
        return Ok(None);
    }

    let login = handle_login_command(
        args,
        LoginCommandDependencies {
            auth_storage: None,
            io: None,
            use_browser_cli: None,
            open_browser_cli_url: None,
        },
    )
    .await?;
    if login.handled {
        return Ok(Some(login.exit_code.unwrap_or(0)));
    }

    let control = handle_control_command(args, &CliControlDependencies)?;
    if control.handled {
        return Ok(Some(control.exit_code.unwrap_or(0)));
    }

    let debug = handle_debug_command(
        args,
        &CliDebugDependencies {
            agent_dir: agent_dir.clone(),
        },
    )
    .await?;
    if debug.handled {
        return Ok(Some(debug.exit_code.unwrap_or(0)));
    }

    let sessions = handle_sessions_command(args, &CliSessionsDependencies).await?;
    if sessions.handled {
        return Ok(Some(sessions.exit_code.unwrap_or(0)));
    }

    let mut parsed = parse_args(args);
    let mut env: BTreeMap<String, String> = pi_js_compat::process_env::vars().into_iter().collect();
    let handoff = if parsed.no_supervisor == Some(true) {
        None
    } else {
        consume_self_restart_request(&mut env, std::process::id(), parent_process_id())
    };
    apply_self_restart_request(&mut parsed, handoff.as_ref());
    wait_for_self_restart_parent_exit(handoff.as_ref(), &SelfRestartWaitDependencies::default()).await;
    if handoff.as_ref().and_then(|handoff| handoff.old_pid) == Some(std::process::id()) {
        prepare_control_db_for_self_restart(&get_control_db_path(&get_user_state_root()?), std::process::id())
            .map_err(CallbackError::from)?;
    }

    let runtime_extension_factories: Arc<Mutex<Vec<InlineExtension>>> = Arc::new(Mutex::new(Vec::new()));
    let interactive_agent_view_selector: Arc<
        Mutex<Option<crate::bundled_extensions::agents_core::runtime::SelectAgentView>>,
    > = Arc::new(Mutex::new(None));
    let orchestration = (parsed.no_supervisor != Some(true)).then(|| FirstPartyOrchestration {
        store: create_first_party_multi_agent_store(),
        runtime_handles: crate::bundled_extensions::agents_core::runtime::create_multi_agent_runtime_handles(),
        select_agent_view: Arc::clone(&interactive_agent_view_selector),
    });
    let first_party_extension_factories = create_first_party_extension_factories(
        orchestration.clone(),
        Arc::clone(&fetch),
        agent_dir.clone(),
        Arc::new({
            let factories = Arc::clone(&runtime_extension_factories);
            move || factories.lock().expect("runtime extension factories").clone()
        }),
    );
    let extension_factories = if parsed.no_extensions.is_some_and(|value| value) {
        options.extension_factories.clone()
    } else {
        let mut factories = first_party_extension_factories;
        factories.extend(options.extension_factories.clone());
        factories
    };
    *runtime_extension_factories.lock().expect("runtime extension factories") = extension_factories.clone();

    if !parsed.diagnostics.is_empty() {
        let chalk = Chalk::stderr();
        for diagnostic in &parsed.diagnostics {
            let message = match diagnostic.kind {
                crate::cli::args::DiagnosticType::Error => chalk.red(&format!("Error: {}", diagnostic.message)),
                crate::cli::args::DiagnosticType::Warning => chalk.yellow(&format!("Warning: {}", diagnostic.message)),
            };
            console_error(&message);
        }
        if parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == crate::cli::args::DiagnosticType::Error)
        {
            return Ok(Some(1));
        }
    }
    time(&"parseArgs".into(), TimingLabel::Main);

    if parsed.version.is_some_and(|value| value) {
        console_log(&package_config().version);
        return Ok(Some(0));
    }

    if let Some(export_path) = parsed.export.as_deref() {
        let output_path = parsed.messages.first().cloned();
        let options = output_path.map(ExportOptions::from_output_path);
        let result = export_from_file(&export_path.into(), options)
            .await
            .map_err(|error| error.message())?;
        console_log(js_format!("Exported to: {}", result));
        return Ok(Some(0));
    }

    if parsed.refresh_models == Some(true) {
        let result = refresh_openrouter_catalog(offline_mode, true, Arc::clone(&fetch))
            .await
            .map_err(CallbackError::from)?;
        console_log(format_model_catalog_refresh_summary(&result));
        return Ok(Some(0));
    }
    load_openrouter_catalog_at_startup()
        .await
        .map_err(CallbackError::from)?;

    let mut app_mode = resolve_app_mode(&parsed, std::io::stdin().is_terminal(), std::io::stdout().is_terminal());
    let should_take_over_stdout = app_mode != AppMode::Interactive && !is_plain_runtime_metadata_command(&parsed);

    if parsed.mode == Some(Mode::Rpc) && !parsed.file_args.is_empty() {
        console_error(&Chalk::stderr().red("Error: @file arguments are not supported in RPC mode"));
        return Ok(Some(1));
    }

    if let Err(MainExit(code)) = validate_fork_flags(&parsed) {
        return Ok(Some(code));
    }
    if let Err(MainExit(code)) = validate_session_id_flags(&parsed) {
        return Ok(Some(code));
    }
    if let Some(worktree) = parsed.worktree.as_deref() {
        cwd = resolve_worktree(&worktree.into(), &cwd).map_err(CallbackError::from)?;
    }

    let migrations = run_migrations(&cwd)?;
    time(&"runMigrations".into(), TimingLabel::Main);

    let mut startup_settings_manager = SettingsManager::create(
        &cwd,
        Some(&agent_dir),
        SettingsManagerCreateOptions { project_trusted: None },
    )
    .map_err(CallbackError::from)?;
    report_diagnostics(&collect_settings_diagnostics(
        &mut startup_settings_manager,
        "startup session lookup",
    ));

    if app_mode == AppMode::Interactive
        && parsed.help != Some(true)
        && parsed.list_models.is_none()
        && crate::cli::startup_ui::should_run_first_time_setup_for_process()?
    {
        let env: BTreeMap<String, String> = pi_js_compat::process_env::vars().into_iter().collect();
        crate::cli::startup_ui::show_first_time_setup(
            &mut startup_settings_manager,
            Rc::new(RefCell::new(pi_tui::terminal::ProcessTerminal::new())),
            &env,
        )
        .await?;
    }

    let env_session_dir = pi_js_compat::process_env::get(&crate::config::env_session_dir());
    let session_dir = match parsed.session_dir.as_deref() {
        Some(dir) => Some(normalize_session_dir(dir)?),
        None => match env_session_dir {
            Some(dir) => Some(JsString::from(crate::config::expand_tilde_path(&dir)?)),
            None => startup_settings_manager.get_session_dir().map(JsString::from),
        },
    };
    let control_db_path = JsString::from(get_control_db_path(&get_user_state_root()?));
    if parsed.no_supervisor != Some(true) {
        crate::core::empty_session_cleanup::sweep_abandoned_empty_sessions(&control_db_path)?;
    }
    let mut session_manager = match create_session_manager(
        &parsed,
        &cwd,
        session_dir.as_ref(),
        &startup_settings_manager,
        Some(&control_db_path),
    )
    .await
    {
        Ok(manager) => manager,
        Err(MainFailure::Exit(MainExit(code))) => return Ok(Some(code)),
        Err(MainFailure::Thrown(error)) => return Err(error),
    };
    session_manager
        .set_metadata_control_db_path(
            Some(&control_db_path),
            crate::core::session_manager::SetMetadataControlDbPathOptions {
                index_message_text: None,
                cwd_override: None,
            },
        )
        .map_err(CallbackError::from)?;
    if let Some(issue) = get_missing_session_cwd_issue(&session_manager, &cwd)? {
        if handoff.is_some() {
            session_manager =
                reopen_session_with_cwd(&issue, session_dir.as_ref(), &issue.fallback_cwd, &control_db_path)
                    .map_err(CallbackError::from)?;
        } else if app_mode == AppMode::Interactive {
            let Some(selected_cwd) = prompt_for_missing_session_cwd(&issue, &startup_settings_manager).await? else {
                return Ok(Some(0));
            };
            session_manager = reopen_session_with_cwd(&issue, session_dir.as_ref(), &selected_cwd, &control_db_path)
                .map_err(CallbackError::from)?;
        } else {
            console_error(Chalk::stderr().red(MissingSessionCwdError::new(issue).message()));
            return Ok(Some(1));
        }
    }
    if handoff.is_some() {
        resolve_restart_interrupted_tool_calls(&mut session_manager)?;
    }
    append_self_restart_notice(&mut session_manager, handoff.as_ref(), std::process::id())
        .map_err(CallbackError::from)?;
    if let Some(name) = parsed.name.as_deref() {
        let name = name.trim();
        if name.is_empty() {
            console_error(&Chalk::stderr().red("Error: --name requires a non-empty value"));
            return Ok(Some(1));
        }
        session_manager
            .set_session_name(Some(&JsString::from(name)))
            .map_err(CallbackError::from)?;
    }
    if should_take_over_stdout {
        take_over_stdout();
    }
    time(&"createSessionManager".into(), TimingLabel::Main);
    let control_message = if parsed.no_supervisor != Some(true) && app_mode == AppMode::Interactive {
        claim_latest_incoming_message(&control_db_path).map_err(CallbackError::from)?
    } else {
        None
    };

    let trust_store = Arc::new(ProjectTrustStore::new(&agent_dir).map_err(CallbackError::from)?);
    let trust_prompt_mode = if parsed.help == Some(true)
        || parsed.list_models.is_some()
        || parsed.list_tools == Some(true)
        || parsed.list_extensions == Some(true)
    {
        AppMode::Print
    } else {
        app_mode
    };
    let project_trust_by_cwd = Arc::new(Mutex::new(HashMap::new()));
    let additional_extension_paths = resolve_cli_paths(&cwd, parsed.extensions.as_deref())?.unwrap_or_default();
    let additional_skill_paths = resolve_cli_paths(&cwd, parsed.skills.as_deref())?.unwrap_or_default();
    let additional_prompt_template_paths =
        resolve_cli_paths(&cwd, parsed.prompt_templates.as_deref())?.unwrap_or_default();
    let additional_theme_paths = resolve_cli_paths(&cwd, parsed.themes.as_deref())?.unwrap_or_default();

    let create_runtime = create_runtime_factory(RuntimeFactoryOptions {
        parsed: parsed.clone(),
        agent_dir: agent_dir.clone(),
        startup_default_project_trust: startup_settings_manager.get_default_project_trust(),
        trust_store,
        trust_prompt_mode,
        app_mode,
        project_trust_by_cwd,
        additional_extension_paths,
        additional_skill_paths,
        additional_prompt_template_paths,
        additional_theme_paths,
        extension_factories,
        orchestration: orchestration.clone(),
    });
    time(&"createRuntime".into(), TimingLabel::Main);
    let session_cwd = session_manager.get_cwd().clone();
    let runtime = create_agent_session_runtime(
        create_runtime,
        &session_cwd,
        &agent_dir,
        session_manager,
        parsed.no_supervisor,
    )
    .await?;
    let runtime = runtime;
    if parsed.no_supervisor != Some(true) {
        reconcile_session_runtime_bindings(&control_db_path, &SessionDirectoryOptions::default())
            .map_err(CallbackError::from)?;
        LifecycleCoordinator::reconcile_dead_detached_runtimes(&control_db_path, &now_iso_string(), Some(&agent_dir))
            .map_err(CallbackError::from)?;
    }
    time(&"createAgentSessionRuntime".into(), TimingLabel::Main);

    let (services, session, model_fallback_message, diagnostics) = {
        let runtime = runtime.clone();
        (
            runtime.services().clone(),
            runtime.session().clone(),
            runtime.model_fallback_message(),
            runtime.diagnostics().to_vec(),
        )
    };
    let settings_manager = Arc::clone(&services.settings_manager);
    let model_registry = services.model_registry.clone();
    let resource_loader = Arc::clone(&services.resource_loader);
    let (image_auto_resize, theme, http_idle_timeout_ms) = {
        let settings = settings_manager.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            settings.get_image_auto_resize(),
            settings.get_theme().cloned(),
            settings.get_http_idle_timeout_ms().map_err(CallbackError::from)?,
        )
    };
    let _ = configure_http_dispatcher(http_idle_timeout_ms as f64)?;

    if parsed.help == Some(true) {
        write_stdout(&print_help(&extension_flags_from_loader(&resource_loader)));
        return Ok(Some(0));
    }

    if let Some(search_pattern) = list_models_search(&parsed) {
        let search_pattern = search_pattern.map(JsString::from);
        list_models(&model_registry, search_pattern.as_ref(), &StdioListModelsIo)
            .await
            .map_err(CallbackError::from)?;
        return Ok(Some(0));
    }

    if parsed.list_tools == Some(true) || parsed.list_extensions == Some(true) {
        session
            .bind_extensions(ExtensionBindings::default())
            .await
            .map_err(CallbackError::from)?;
        if parsed.list_tools == Some(true) {
            let tools: Vec<crate::core::extensions::types::ToolInfo> = session
                .get_all_tools()
                .into_iter()
                .map(|tool| crate::core::extensions::types::ToolInfo {
                    name: tool.name,
                    description: tool.description,
                    parameters: tool.parameters,
                    prompt_guidelines: (!tool.prompt_guidelines.is_empty()).then_some(tool.prompt_guidelines),
                    source_info: tool.source_info,
                })
                .collect();
            write_stdout(&list_tools(&tools, &session.get_active_tool_names()));
        } else {
            write_stdout(&list_extensions(&resource_loader.get_extensions().extensions));
        }
        return Ok(Some(0));
    }

    let stdin_content = if app_mode != AppMode::Rpc {
        let content = read_piped_stdin().await?;
        if content.is_some() && app_mode == AppMode::Interactive {
            app_mode = AppMode::Print;
        }
        content
    } else {
        None
    };
    time(&"readPipedStdin".into(), TimingLabel::Main);

    let initial = prepare_initial_message(&mut parsed, image_auto_resize, stdin_content.as_deref()).await?;
    time(&"prepareInitialMessage".into(), TimingLabel::Main);
    init_theme(theme.as_ref(), app_mode == AppMode::Interactive).map_err(CallbackError::from)?;
    time(&"initTheme".into(), TimingLabel::Main);

    if app_mode == AppMode::Interactive && !migrations.deprecation_warnings.is_empty() {
        show_deprecation_warnings(&migrations.deprecation_warnings).await;
    }
    time(&"resolveModelScope".into(), TimingLabel::Main);
    report_diagnostics(&diagnostics);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.diagnostic_type == "error")
    {
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Failed to load extension"))
        {
            console_error(&Chalk::stderr().yellow("Hint: Start without extensions using \"pi -ne\"."));
        }
        return Ok(Some(1));
    }
    time(&"createAgentSession".into(), TimingLabel::Main);

    let no_models_message = format_no_models_available_message()?;
    if app_mode != AppMode::Interactive
        && model_fallback_message
            .as_ref()
            .is_some_and(|message| *message == no_models_message)
    {
        console_error(&Chalk::stderr().red(&no_models_message));
        return Ok(Some(1));
    }

    let startup_benchmark = is_truthy_env_flag(pi_js_compat::process_env::get("PI_STARTUP_BENCHMARK").as_deref());
    if startup_benchmark && app_mode != AppMode::Interactive {
        console_error(&Chalk::stderr().red("Error: PI_STARTUP_BENCHMARK only supports interactive mode"));
        return Ok(Some(1));
    }

    if parsed.no_supervisor != Some(true) {
        let path = control_db_path.clone();
        let settings = startup_settings_manager.get_global_settings();
        tokio::spawn(async move {
            let result = async {
                let relay = crate::core::host_relay_resident::relay_settings_from_global(&settings)?;
                crate::core::host_relay_resident::ensure_relay_running(&path, relay.as_ref()).await
            }
            .await;
            if let Err(error) = result {
                eprintln!("Relay startup failed: {error}");
            }
        });
    }

    if app_mode == AppMode::Rpc {
        print_timings();
        run_rpc_mode(runtime).await?;
        return Ok(None);
    }
    if app_mode == AppMode::Interactive {
        let auto_trust_on_reload_cwd = if parsed.project_trust_override.is_none()
            && !has_trust_requiring_project_resources(&session_cwd).map_err(CallbackError::from)?
        {
            Some(session_cwd.clone())
        } else {
            None
        };
        let controls = create_interactive_orchestration_controls(orchestration.as_ref(), control_db_path.clone());
        let options = InteractiveModeOptions {
            migrated_providers: migrations.migrated_auth_providers.clone(),
            model_fallback_message: model_fallback_message.clone(),
            auto_trust_on_reload_cwd,
            initial_message: initial.initial_message.clone(),
            initial_images: initial.initial_images.clone().unwrap_or_default(),
            initial_messages: parsed.messages.iter().map(JsString::from).collect(),
            control_message,
            verbose: parsed.verbose.is_some_and(|value| value),
            control_db_path: Some(control_db_path.clone()),
            multi_agent_store: orchestration
                .as_ref()
                .map(|orchestration| Arc::clone(&orchestration.store)),
            wake_wait_agents_after_steering: controls.wake_wait_agents_after_steering,
            steer_multi_agent: Some(controls.steer_multi_agent),
            cancel_multi_agent: controls.cancel_multi_agent,
            agent_view_selector_slot: Some(Arc::clone(&interactive_agent_view_selector)),
        };
        let local = tokio::task::LocalSet::new();
        let code = local
            .run_until(async move {
                use crate::modes::interactive::interactive_mode::{
                    ResolvedMutationTarget, bind_interactive_mode_session_mutation_target_resolver,
                };
                let mode = InteractiveMode::new(runtime, options, None).await?;
                bind_interactive_mode_session_mutation_target_resolver(
                    &mode,
                    Arc::new(move || match &orchestration {
                        Some(orchestration) => {
                            crate::bundled_extensions::agents_core::runtime::resolve_selected_session_mutation_target(
                                &orchestration.store,
                                &orchestration.runtime_handles,
                            )
                            .map(|target| target.map(ResolvedMutationTarget::Interactive))
                        }
                        None => Ok(None),
                    }),
                );
                if startup_benchmark {
                    let result = run_interactive_action_or_report_terminal_error(|| mode.init(), None).await?;
                    if !result.ok {
                        return Ok::<Option<i32>, CallbackError>(Some(result.exit_code.unwrap_or(1)));
                    }
                    // Allow terminal query replies before restoring the terminal.
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    mode.stop()?;
                    stop_theme_watcher();
                    print_timings();
                } else {
                    print_timings();
                    let result = run_interactive_action_or_report_terminal_error(|| mode.run(), None).await?;
                    if !result.ok {
                        return Ok(Some(result.exit_code.unwrap_or(1)));
                    }
                }
                Ok(None)
            })
            .await?;
        return Ok(code);
    }

    print_timings();
    let print_mode = match to_print_output_mode(app_mode) {
        Mode::Json => PrintMode::Json,
        _ => PrintMode::Text,
    };
    let result = run_print_mode(
        runtime,
        PrintModeOptions {
            mode: print_mode,
            messages: parsed.messages.iter().map(JsString::from).collect(),
            initial_message: initial.initial_message,
            initial_images: initial.initial_images.unwrap_or_default(),
        },
    )
    .await?;
    stop_theme_watcher();
    restore_stdout();
    Ok((result != 0).then_some(result))
}
