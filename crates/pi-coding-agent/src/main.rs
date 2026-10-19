//! Port of `packages/coding-agent/src/main.ts`.
//!
//! This module mirrors the startup helpers of the TypeScript entry point.
//! The orchestration (`main`) is added once the in-flight mode/runtime slices
//! land on `master`; see `docs/porting/traceability/main.md` for the current
//! mapping and the explicit gaps.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use pi_ai::auth::types::{AuthOperationOptions, CredentialStore};
use pi_ai::models::models_are_equal;
use tokio_util::sync::CancellationToken;

use crate::cli::args::{
    Args, ExtensionFlag, ExtensionFlagDefault, ExtensionFlagType, ListModels, Mode, parse_args, print_help,
};
use crate::cli::auth_check::{
    AuthCheckReason, AuthCheckResult, AuthCheckStatus, CheckProviderAuthOptions, check_provider_auth,
    create_auth_check_model_runtime, get_provider_credential,
};
use crate::cli::config_selector::TuiConfigSelector;
use crate::cli::credential_print::{CredentialPrintKind, resolve_credential_for_print};
use crate::cli::file_processor::{ProcessFileOptions, process_file_arguments};
use crate::cli::initial_message::{InitialMessageInput, InitialMessageResult, build_initial_message};
use crate::cli::list_extensions::list_extensions;
use crate::cli::list_models::list_models;
use crate::cli::list_tools::list_tools;
use crate::cli::project_trust::{CreateProjectTrustContextOptions, create_project_trust_context};
use crate::cli::resident_console_command::parse_resident_console_args;
use crate::cli::{
    AuthCommand, AuthCommandError, AuthCommandKind, AuthCommandTarget, ControlCommandDependencies,
    DebugCommandDependencies, LoginCommandDependencies, SessionsCommandDependencies, get_auth_command_name,
    get_auth_command_usage, handle_control_command, handle_debug_command, handle_login_command,
    handle_sessions_command, is_auth_command_help, parse_auth_command, print_auth_command_help,
    validate_auth_command_args,
};
use crate::config::{InstallContext, get_agent_dir, get_package_dir, get_user_state_root, package_config};
use crate::core::agent_session::{ExtensionBindings, MultiAgentRuntimeRole, create_multi_agent_execution_capability};
use crate::core::agent_session_runtime::{
    CreateAgentSessionRuntimeFactory, CreateAgentSessionRuntimeOptions, CreateAgentSessionRuntimeResult,
    create_agent_session_runtime,
};
use crate::core::agent_session_services::{
    AgentSessionRuntimeDiagnostic, AgentSessionServices, CreateAgentSessionFromServicesOptions,
    CreateAgentSessionServicesOptions, create_agent_session_from_services, create_agent_session_services,
};
use crate::core::auth_guidance::format_no_models_available_message;
use crate::core::auth_storage::{AuthStorage, ReadOnlyAuthStorage};
use crate::core::export_html::{ExportOptions, export_from_file};
use crate::core::extensions::loader::ExtensionFactory;
use crate::core::external_session_importer::{
    ExternalSessionImportOptions, import_external_session_alias, is_external_session_alias,
};
use crate::core::http_dispatcher::configure_http_dispatcher;
use crate::core::lifecycle_coordinator::LifecycleCoordinator;
use crate::core::model_resolver::{ResolveCliModelOptions, ScopedModel, resolve_cli_model, resolve_model_scope};
use crate::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use crate::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use crate::core::output_guard::{restore_stdout, take_over_stdout};
use crate::core::project_trust::{AppMode, ResolveProjectTrustedOptions, resolve_project_trusted};
use crate::core::resource_loader::{DefaultResourceLoaderOptions, InlineExtension, ResourceLoader};
use crate::core::sdk::{CreateAgentSessionOptions, NoTools};
use crate::core::self_restart::{
    SelfRestartWaitDependencies, append_self_restart_notice, apply_self_restart_request, consume_self_restart_request,
    wait_for_self_restart_parent_exit,
};
use crate::core::session_control_db::{
    claim_latest_incoming_message, get_control_db_path, prepare_control_db_for_self_restart,
};
use crate::core::session_cwd::{MissingSessionCwdError, get_missing_session_cwd_issue, reopen_session_with_cwd};
use crate::core::session_directory::{SessionDirectoryOptions, reconcile_session_runtime_bindings};
use crate::core::session_manager::{
    NewSessionOptions, SessionInfo, SessionManager, assert_valid_session_id, get_default_session_dir,
};
use crate::core::settings_manager::{DefaultProjectTrust, SettingsManager, SettingsManagerCreateOptions};
use crate::core::timings::{TimingLabel, print_timings, reset_timings, time};
use crate::core::trust_manager::{ProjectTrustDecision, ProjectTrustStore, has_trust_requiring_project_resources};
use crate::migrations::{run_migrations, show_deprecation_warnings};
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
use crate::utils::paths::{PathInputOptions, is_local_path, normalize_path, resolve_path};
use crate::utils::windows_self_update::cleanup_windows_self_update_quarantine;

/// `process.exit(code)` represented as an error so helpers can stop the flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MainExit(pub i32);

/// Result of a helper that may call `process.exit`.
pub type MainResult<T> = Result<T, MainExit>;

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
    "self-restart",
    "session-id",
];

/// First-party extensions whose TypeScript factories are not ported on this
/// branch yet. `main.ts` registers every name in this list unconditionally;
/// the registry wiring lands with each extension slice.
pub const PENDING_FIRST_PARTY_EXTENSIONS: &[&str] = &[
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
    "default-footer",
    "docs-tree-context",
    "effort",
    "loop",
    "openai-remote-compact",
    "run-plan",
    "safe",
    "spec-validation",
    "session-archive",
    "self-restart",
    "session-id",
];

/// `firstPartyExtensionFactory`.
pub fn first_party_extension_factory(name: &str, factory: ExtensionFactory) -> InlineExtension {
    InlineExtension::Factory {
        extension_path: Some(format!("<first-party:{name}>")),
        factory,
    }
}

/// `createFirstPartyExtensionFactories` for the bundled extensions ported on
/// this branch. The shared store and runtime handles are created by `main`.
pub fn create_first_party_extension_factories(
    store: Arc<MultiAgentStore>,
    runtime_handles: crate::bundled_extensions::agents_core::runtime::MultiAgentRuntimeHandles,
) -> Vec<InlineExtension> {
    let options = crate::bundled_extensions::agents_core::runtime::MultiAgentExtensionOptions {
        runtime_handles: Some(runtime_handles),
        store: Some(Arc::clone(&store)),
        ..Default::default()
    };
    let mut factories = crate::bundled_extensions::agents_extension_factories(&options)
        .into_iter()
        .map(|(name, factory)| first_party_extension_factory(name, factory))
        .collect::<Vec<_>>();
    factories.push(first_party_extension_factory(
        "goal",
        crate::bundled_extensions::goal::goal_extension_factory(
            crate::bundled_extensions::goal::goal_types::GoalExtensionOptions::default(),
        ),
    ));
    factories.push(first_party_extension_factory(
        "bwrap",
        crate::bundled_extensions::bwrap::bwrap_extension_factory(
            crate::bundled_extensions::bwrap::BwrapExtensionOptions::default(),
        ),
    ));
    factories.push(first_party_extension_factory("pyrun", pyrun_extension_factory(options)));
    factories
}

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
                    let value = serde_json::json!({
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
    pub agent_dir: String,
    pub startup_default_project_trust: DefaultProjectTrust,
    pub trust_store: Arc<ProjectTrustStore>,
    pub trust_prompt_mode: AppMode,
    pub app_mode: AppMode,
    pub project_trust_by_cwd: Arc<Mutex<HashMap<String, bool>>>,
    pub additional_extension_paths: Vec<String>,
    pub additional_skill_paths: Vec<String>,
    pub additional_prompt_template_paths: Vec<String>,
    pub additional_theme_paths: Vec<String>,
    pub extension_factories: Vec<InlineExtension>,
    pub multi_agent_store: Arc<MultiAgentStore>,
    pub multi_agent_runtime_handles: crate::bundled_extensions::agents_core::runtime::MultiAgentRuntimeHandles,
}

fn trust_decision_is_trusted(store: &ProjectTrustStore, cwd: &str) -> bool {
    store.get(cwd) == Ok(ProjectTrustDecision::Trusted)
}

/// Runtime reason strings mapped to `session_start` metadata.
fn session_start_event(
    reason: &str,
    previous_session_file: Option<String>,
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
    cwd: &str,
    mode: AppMode,
    has_ui: bool,
) -> crate::core::extensions::types::ProjectTrustContext {
    create_project_trust_context(CreateProjectTrustContextOptions {
        cwd: cwd.to_owned(),
        mode,
        has_ui,
        startup_ui: None,
    })
}

async fn create_runtime(
    options: &RuntimeFactoryOptions,
    request: CreateAgentSessionRuntimeOptions,
) -> Result<CreateAgentSessionRuntimeResult, String> {
    let mut session_manager = request.session_manager;
    let parsed = &options.parsed;
    session_manager
        .set_metadata_control_db_path(
            Some(&get_control_db_path(&get_user_state_root())),
            crate::core::session_manager::SetMetadataControlDbPathOptions {
                index_message_text: None,
                cwd_override: None,
            },
        )
        .map_err(|error| error.to_string())?;
    options
        .multi_agent_store
        .restore_from_session_manager(&session_manager)
        .map_err(|error| error.to_string())?;

    let is_initial_runtime = request.session_start_reason == "startup";
    let project_trust_diagnostics = Arc::new(Mutex::new(Vec::new()));
    let cwd = request.cwd.clone();
    let cached_project_trust = options
        .project_trust_by_cwd
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&cwd)
        .copied();
    let has_trust_requiring_resources = has_trust_requiring_project_resources(&cwd);
    let should_resolve_project_trust =
        parsed.project_trust_override.is_none() && cached_project_trust.is_none() && has_trust_requiring_resources;
    let project_trusted = if should_resolve_project_trust {
        false
    } else {
        cached_project_trust
            .or(parsed.project_trust_override)
            .unwrap_or(!has_trust_requiring_resources || trust_decision_is_trusted(&options.trust_store, &cwd))
    };

    let runtime_settings_manager = SettingsManager::create(
        &cwd,
        Some(Path::new(&options.agent_dir)),
        SettingsManagerCreateOptions {
            project_trusted: Some(project_trusted),
        },
    )
    .map_err(|error| error.to_string())?;

    let timeout = TimeoutSignal::new(15_000);
    let reload_options = if should_resolve_project_trust {
        let trust_store = Arc::clone(&options.trust_store);
        let project_trust_by_cwd = Arc::clone(&options.project_trust_by_cwd);
        let diagnostics = Arc::clone(&project_trust_diagnostics);
        let trust_override = parsed.project_trust_override;
        let default_project_trust = options.startup_default_project_trust;
        let trust_prompt_mode = options.trust_prompt_mode;
        let app_mode = options.app_mode;
        let resolver_cwd = cwd.clone();
        Some(Arc::new(
            move |extensions_result: crate::core::extensions::types::LoadExtensionsResult| {
                let trust_store = Arc::clone(&trust_store);
                let project_trust_by_cwd = Arc::clone(&project_trust_by_cwd);
                let diagnostics = Arc::clone(&diagnostics);
                let cwd = resolver_cwd.clone();
                Box::pin(async move {
                    let mode = if is_initial_runtime {
                        trust_prompt_mode
                    } else {
                        app_mode
                    };
                    let context = project_trust_context(
                        &cwd,
                        mode,
                        is_initial_runtime && trust_prompt_mode == AppMode::Interactive,
                    );
                    let on_extension_error = |message: String| {
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
                    .map_err(|error| error.to_string())?;
                    project_trust_by_cwd
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .insert(cwd, trusted);
                    Ok(trusted)
                }) as pi_ai::request::BoxFuture<'static, Result<bool, String>>
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
        system_prompt: parsed.system_prompt.clone(),
        append_system_prompt: parsed.append_system_prompt.clone(),
        ..Default::default()
    };

    let services = create_agent_session_services(CreateAgentSessionServicesOptions {
        cwd: cwd.clone(),
        agent_dir: Some(options.agent_dir.clone()),
        settings_manager: Some(runtime_settings_manager),
        model_runtime: None,
        model_runtime_signal: Some(timeout.token()),
        extension_flag_values: Some(parsed.unknown_flags.clone()),
        resource_loader_options: Some(resource_loader_options),
        resource_loader: None,
        resource_loader_reload_options: reload_options,
    })
    .await?;

    let settings_manager = Arc::clone(&services.settings_manager);
    let resource_loader = Arc::clone(&services.resource_loader);
    let model_runtime = services.model_runtime.clone();
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
            message: format!("Failed to load extension \"{}\": {}", error.path, error.error),
        }
    }));

    let model_patterns = parsed.models.clone().or_else(|| {
        settings_manager
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get_enabled_models()
    });
    let scoped_models = match model_patterns.filter(|patterns| !patterns.is_empty()) {
        Some(patterns) => {
            resolve_model_scope(
                &patterns,
                &model_runtime,
                AuthOperationOptions::new(Some(timeout.token())),
            )
            .await
        }
        None => Vec::new(),
    };

    let has_existing_session = !session_manager.build_session_context().messages.is_empty();
    let (session_options, cli_thinking_from_model, option_diagnostics) = {
        let settings_guard = settings_manager.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let built = build_session_options(
            parsed,
            &scoped_models,
            has_existing_session,
            &model_runtime,
            &settings_guard,
        );
        (built.options, built.cli_thinking_from_model, built.diagnostics)
    };
    diagnostics.extend(option_diagnostics);

    if let Some(api_key) = parsed.api_key.as_deref() {
        match session_options.model.as_ref() {
            None => diagnostics.push(AgentSessionRuntimeDiagnostic {
                diagnostic_type: "error".to_owned(),
                message: "--api-key requires a model to be specified via --model, --provider/--model, or --models"
                    .to_owned(),
            }),
            Some(model) => {
                let provider = model.provider.clone();
                model_runtime
                    .set_runtime_api_key(&provider, api_key, AuthOperationOptions::default())
                    .await
                    .map_err(|error| error.to_string())?;
                model_runtime
                    .refresh(pi_ai::models::ModelsRefreshOptions {
                        allow_network: Some(false),
                        providers: Some(vec![provider]),
                        ..Default::default()
                    })
                    .await
                    .map_err(|error| error.to_string())?;
            }
        }
    }

    let result = create_agent_session_from_services(CreateAgentSessionFromServicesOptions {
        services: AgentSessionServices {
            cwd: services.cwd.clone(),
            agent_dir: services.agent_dir.clone(),
            model_runtime: services.model_runtime.clone(),
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
        multi_agent_runtime_role: Some(MultiAgentRuntimeRole::Orchestrator),
        multi_agent_execution_capability: Some(create_multi_agent_execution_capability()),
        custom_tools: Vec::new(),
        base_tools: Vec::new(),
        multi_agent_store: Some(Arc::clone(&options.multi_agent_store)),
        on_shared_channel_message_delivered: {
            let handles = options.multi_agent_runtime_handles.clone();
            Some(Arc::new(move |prompt: String| {
                crate::bundled_extensions::agents_core::runtime::wake_wait_agents_after_coordination(&handles, &prompt);
            }))
        },
        session_start_event: Some(session_start_event(
            &request.session_start_reason,
            request.previous_session_file.clone(),
        )),
    })
    .await?;

    let cli_thinking_override = parsed.thinking.is_some() || cli_thinking_from_model;
    if cli_thinking_override {
        result
            .session
            .set_thinking_level(result.session.thinking_level())
            .map_err(|error| error.message().to_owned())?;
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

/// Writes raw bytes to stdout and flushes, like `process.stdout.write`.
pub fn write_stdout(text: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
}

/// Writes raw bytes to stderr and flushes, like `process.stderr.write`.
pub fn write_stderr(text: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(text.as_bytes());
    let _ = stderr.flush();
}

/// `console.log`.
pub fn console_log(text: &str) {
    write_stdout(&format!("{text}\n"));
}

/// `console.error`.
pub fn console_error(text: &str) {
    write_stderr(&format!("{text}\n"));
}

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
            || parsed.list_tools == Some(true)
            || parsed.list_extensions == Some(true))
}

/// `isTruthyEnvFlag(process.env.PI_OFFLINE)`.
pub fn offline_mode_from_env(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--offline") || is_truthy_env_flag(std::env::var("PI_OFFLINE").ok().as_deref())
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
            message: format!("({context}, {} settings) {}", error.scope.as_str(), error.message),
        })
        .collect()
}

/// `reportDiagnostics`.
pub fn report_diagnostics(diagnostics: &[AgentSessionRuntimeDiagnostic]) {
    let chalk = Chalk::stderr();
    for diagnostic in diagnostics {
        let message = match diagnostic.diagnostic_type.as_str() {
            "error" => chalk.red(&format!("Error: {}", diagnostic.message)),
            "warning" => chalk.yellow(&format!("Warning: {}", diagnostic.message)),
            _ => chalk.dim(&diagnostic.message),
        };
        console_error(&message);
    }
}

/// `refreshRpcModelCatalogs`.
pub fn refresh_rpc_model_catalogs(model_runtime: &ModelRuntime, offline_mode: bool, app_mode: AppMode) {
    if offline_mode || app_mode != AppMode::Rpc {
        return;
    }
    let runtime = model_runtime.clone();
    tokio::spawn(async move {
        if let Err(error) = runtime.refresh(Default::default()).await {
            report_diagnostics(&[AgentSessionRuntimeDiagnostic {
                diagnostic_type: "warning".to_owned(),
                message: format!("Failed to refresh model catalogs: {error}"),
            }]);
        }
    });
}

// =============================================================================
// Abort helpers
// =============================================================================

/// `AbortSignal.timeout(ms)` as a cancellation token. The timer is aborted when
/// the returned guard is dropped.
pub struct TimeoutSignal {
    signal: CancellationToken,
    timer: tokio::task::JoinHandle<()>,
}

impl TimeoutSignal {
    pub fn new(timeout_ms: u64) -> Self {
        let signal = CancellationToken::new();
        let timer_signal = signal.clone();
        let timer = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)).await;
            timer_signal.cancel();
        });
        Self { signal, timer }
    }

    pub fn token(&self) -> CancellationToken {
        self.signal.clone()
    }
}

impl Drop for TimeoutSignal {
    fn drop(&mut self) {
        self.timer.abort();
    }
}

// =============================================================================
// Auth command
// =============================================================================

fn auth_check_result_json(result: &AuthCheckResult, credential: Option<&str>) -> String {
    let mut object = serde_json::Map::new();
    object.insert(
        "status".to_owned(),
        serde_json::Value::String(result.status.as_str().to_owned()),
    );
    object.insert(
        "provider".to_owned(),
        serde_json::Value::String(result.provider.clone()),
    );
    if let Some(reason) = result.reason {
        object.insert(
            "reason".to_owned(),
            serde_json::Value::String(reason.as_str().to_owned()),
        );
    }
    if let Some(auth_type) = result.auth_type {
        object.insert(
            "authType".to_owned(),
            serde_json::Value::String(auth_type.as_str().to_owned()),
        );
    }
    if let Some(credential) = credential {
        object.insert(
            "credentials".to_owned(),
            serde_json::Value::String(credential.to_owned()),
        );
    }
    serde_json::Value::Object(object).to_string()
}

fn credential_print_kind(kind: AuthCommandKind) -> Option<CredentialPrintKind> {
    match kind {
        AuthCommandKind::Check => None,
        AuthCommandKind::ApiKey => Some(CredentialPrintKind::ApiKey),
        AuthCommandKind::BearerToken => Some(CredentialPrintKind::BearerToken),
    }
}

async fn check_auth_with_credentials(
    command: &AuthCommand,
    parsed: &Args,
    requested_auth: &AuthCommandTarget,
) -> (AuthCheckResult, Option<String>) {
    let credentials: std::sync::Arc<dyn CredentialStore> = if command.no_refresh() {
        std::sync::Arc::new(ReadOnlyAuthStorage::new(None))
    } else {
        std::sync::Arc::new(AuthStorage::create(None))
    };
    let Ok(model_runtime) = create_auth_check_model_runtime(credentials.clone()).await else {
        return (invalid_auth_result(requested_auth), None);
    };
    let Ok(result) = check_provider_auth(
        parsed,
        &model_runtime,
        CheckProviderAuthOptions {
            refresh: !command.no_refresh(),
        },
    )
    .await
    else {
        return (invalid_auth_result(requested_auth), None);
    };
    if !command.credentials() || result.status != AuthCheckStatus::Ready {
        return (result, None);
    }

    match get_provider_credential(
        &result.provider,
        &model_runtime,
        credentials,
        CheckProviderAuthOptions {
            refresh: !command.no_refresh(),
        },
    )
    .await
    {
        Ok(Some(credential)) => (result, Some(credential)),
        Ok(None) => (
            AuthCheckResult {
                status: AuthCheckStatus::NotReady,
                provider: result.provider,
                reason: Some(AuthCheckReason::CredentialNotAvailable),
                auth_type: None,
            },
            None,
        ),
        Err(_) => (invalid_auth_result(requested_auth), None),
    }
}

async fn run_auth_check_command(
    command: &AuthCommand,
    parsed: &Args,
    requested_auth: &AuthCommandTarget,
    exit_code: &mut i32,
) {
    let (result, credential) = check_auth_with_credentials(command, parsed, requested_auth).await;
    let output = if command.json() {
        auth_check_result_json(&result, credential.as_deref())
    } else {
        credential.unwrap_or_else(|| result.status.as_str().to_owned())
    };
    console_log(&output);
    *exit_code = match result.status {
        AuthCheckStatus::Ready => 0,
        AuthCheckStatus::NotReady => 1,
        AuthCheckStatus::Invalid => 2,
    };
}

fn invalid_auth_result(requested_auth: &AuthCommandTarget) -> AuthCheckResult {
    AuthCheckResult {
        status: AuthCheckStatus::Invalid,
        provider: requested_auth
            .provider()
            .map(str::to_owned)
            .or_else(|| requested_auth.model().map(str::to_owned))
            .unwrap_or_default(),
        reason: Some(AuthCheckReason::InvalidState),
        auth_type: None,
    }
}

/// `runAuthCommand`. `Ok(false)` means the arguments are not an auth command.
pub async fn run_auth_command(args: &[String], exit_code: &mut i32) -> bool {
    if is_auth_command_help(args) {
        write_stdout(&print_auth_command_help());
        return true;
    }

    let command = match parse_auth_command(args) {
        Ok(Some(command)) => command,
        Ok(None) => return false,
        Err(error) => {
            let message = error.message();
            console_error(&Chalk::stderr().red(&format!("Error: {message}")));
            *exit_code = 1;
            return true;
        }
    };

    let parsed = parse_args(command.args());
    if !parsed.unknown_flags.is_empty() {
        let option = parsed.unknown_flags.keys().next().expect("non-empty");
        console_error(&Chalk::stderr().red(&format!(
            "Unknown option --{option} for \"{}\".",
            get_auth_command_name(command.kind())
        )));
        console_error(&Chalk::stderr().dim(&format!(
            "Use \"{} --help\" or \"{}\".",
            package_config().app_name,
            get_auth_command_usage(command.kind())
        )));
        *exit_code = 1;
        return true;
    }

    let result = run_auth_command_inner(&command, &parsed, exit_code).await;
    if let Err(message) = result {
        console_error(&Chalk::stderr().red(&format!("Error: {message}")));
        *exit_code = if command.kind() == AuthCommandKind::Check { 2 } else { 1 };
    }
    true
}

async fn run_auth_command_inner(command: &AuthCommand, parsed: &Args, exit_code: &mut i32) -> Result<(), String> {
    if !parsed.diagnostics.is_empty() {
        return Err(parsed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>()
            .join("\n"));
    }

    if let Some(kind) = credential_print_kind(command.kind()) {
        let timeout = TimeoutSignal::new(15_000);
        let model_runtime = ModelRuntime::create(CreateModelRuntimeOptions {
            allow_model_network: false,
            signal: Some(timeout.token()),
            ..Default::default()
        })
        .await
        .map_err(|error| error.to_string())?;
        let credential = resolve_credential_for_print(
            parsed,
            &model_runtime,
            kind,
            command.min_expiry_ms(),
            Some(timeout.token()),
        )
        .await
        .map_err(|error| error.message().to_owned())?;
        console_log(&credential);
        return Ok(());
    }

    let requested_auth = validate_auth_command_args(parsed, command.kind())
        .map_err(|error: AuthCommandError| error.message().to_owned())?;
    run_auth_check_command(command, parsed, &requested_auth, exit_code).await;
    Ok(())
}

// =============================================================================
// Initial message
// =============================================================================

/// `prepareInitialMessage`.
pub async fn prepare_initial_message(
    parsed: &mut Args,
    auto_resize_images: bool,
    stdin_content: Option<&str>,
) -> Result<InitialMessageResult, String> {
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
        &process_cwd(),
    )
    .await
    .map_err(|error| error.message().to_owned())?;
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
    Path(String),
    Local(String),
    Global { path: String, cwd: String },
    NotFound(String),
}

fn process_cwd() -> String {
    std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

fn resolve_from_cwd(input: &str) -> Result<String, String> {
    resolve_path(input, &process_cwd(), &PathInputOptions::default()).map_err(|error| error.to_string())
}

/// `findLocalSessionByExactId`.
pub fn find_local_session_by_exact_id(
    session_id: &str,
    cwd: &str,
    session_dir: Option<&str>,
    control_db_path: Option<&str>,
) -> Result<Option<ResolvedSession>, String> {
    let local_sessions =
        SessionManager::list(cwd, session_dir, None, control_db_path).map_err(|error| error.to_string())?;
    Ok(local_sessions
        .into_iter()
        .find(|session| session.id == session_id)
        .map(|session| ResolvedSession::Local(session.path)))
}

/// `sessionBelongsToProject`.
pub fn session_belongs_to_project(session_cwd: &str, cwd: &str) -> bool {
    if let (Ok(session), Ok(current)) = (resolve_from_cwd(session_cwd), resolve_from_cwd(cwd))
        && session == current
    {
        return true;
    }

    let session_git_directory = read_git_common_directory(Path::new(session_cwd));
    let current_git_directory = read_git_common_directory(Path::new(cwd));
    session_git_directory.is_some() && session_git_directory == current_git_directory
}

fn find_session_match<'a>(sessions: &'a [SessionInfo], session_arg: &str) -> Option<&'a SessionInfo> {
    sessions
        .iter()
        .find(|session| session.id == session_arg)
        .or_else(|| sessions.iter().find(|session| session.id.starts_with(session_arg)))
}

/// `resolveSessionPath`.
pub fn resolve_session_path(
    session_arg: &str,
    cwd: &str,
    session_dir: Option<&str>,
    control_db_path: Option<&str>,
) -> Result<ResolvedSession, String> {
    let path_like = session_arg.contains('/') || session_arg.contains('\\') || session_arg.ends_with(".jsonl");
    let resolved_session_arg_path = if path_like {
        Some(resolve_path(session_arg, cwd, &PathInputOptions::default()).map_err(|error| error.to_string())?)
    } else {
        None
    };
    if let Some(resolved) = &resolved_session_arg_path
        && (session_arg.ends_with(".jsonl") || Path::new(resolved).exists())
    {
        return Ok(ResolvedSession::Path(resolved.clone()));
    }

    if is_external_session_alias(session_arg) {
        let imported = import_external_session_alias(
            session_arg,
            &ExternalSessionImportOptions {
                home_dir: None,
                session_dir: session_dir.map(str::to_owned),
                fallback_cwd: Some(cwd.to_owned()),
            },
        )
        .map_err(|error| error.to_string())?;
        return Ok(match imported {
            Some(session) => ResolvedSession::Path(session.path),
            None => ResolvedSession::NotFound(session_arg.to_owned()),
        });
    }

    if let Some(resolved) = resolved_session_arg_path {
        return Ok(ResolvedSession::Path(resolved));
    }

    let local_sessions =
        SessionManager::list(cwd, session_dir, None, control_db_path).map_err(|error| error.to_string())?;
    if let Some(local_match) = find_session_match(&local_sessions, session_arg) {
        return Ok(ResolvedSession::Local(local_match.path.clone()));
    }

    let all_sessions =
        SessionManager::list_all(session_dir, None, control_db_path).map_err(|error| error.to_string())?;
    if let Some(global_match) = find_session_match(&all_sessions, session_arg) {
        if session_belongs_to_project(&global_match.cwd, cwd) {
            return Ok(ResolvedSession::Local(global_match.path.clone()));
        }
        return Ok(ResolvedSession::Global {
            path: global_match.path.clone(),
            cwd: global_match.cwd.clone(),
        });
    }

    Ok(ResolvedSession::NotFound(session_arg.to_owned()))
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
pub fn validate_fork_flags(parsed: &Args) -> MainResult<()> {
    let Some(message) = fork_flag_conflict(parsed) else {
        return Ok(());
    };
    console_error(&Chalk::stderr().red(&message));
    Err(MainExit(1))
}

/// `validateSessionIdFlags`.
pub fn validate_session_id_flags(parsed: &Args) -> MainResult<()> {
    let Some(session_id) = parsed.session_id.as_deref() else {
        return Ok(());
    };
    if let Some(message) = session_id_flag_conflict(parsed) {
        console_error(&Chalk::stderr().red(&message));
        return Err(MainExit(1));
    }
    if let Err(error) = assert_valid_session_id(session_id) {
        console_error(&Chalk::stderr().red(&format!("Error: {error}")));
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
    Fut: Future<Output = Result<(), String>>,
{
    match action().await {
        Ok(()) => 0,
        Err(message) => {
            console_error(&Chalk::stderr().red(&format!("Error: {message}")));
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

#[derive(Debug)]
struct MessageError(String);

impl std::fmt::Display for MessageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for MessageError {}

/// `runInteractiveActionOrReportTerminalError`.
pub async fn run_interactive_action_or_report_terminal_error<F, Fut>(
    action: F,
    on_terminal_error: Option<&dyn Fn()>,
) -> Result<InteractiveActionResult, String>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    match action().await {
        Ok(()) => Ok(InteractiveActionResult {
            ok: true,
            exit_code: None,
        }),
        Err(message) => {
            crate::modes::interactive::theme::stop_theme_watcher();
            let error = MessageError(message);
            if pi_tui::is_terminal_raw_mode_failure(&error) {
                console_error(&Chalk::stderr().red(&format!("Error: {error}")));
                if let Some(on_terminal_error) = on_terminal_error {
                    on_terminal_error();
                }
                return Ok(InteractiveActionResult {
                    ok: false,
                    exit_code: Some(1),
                });
            }
            Err(error.0)
        }
    }
}

fn open_session_or_exit(path: &str, session_dir: Option<&str>) -> MainResult<SessionManager> {
    SessionManager::open(path, session_dir, None).map_err(|error| {
        console_error(&Chalk::stderr().red(&format!("Error: {error}")));
        MainExit(1)
    })
}

fn fork_session_or_exit(
    source_path: &str,
    cwd: &str,
    session_dir: Option<&str>,
    session_id: Option<&str>,
) -> MainResult<SessionManager> {
    SessionManager::fork_from(
        source_path,
        cwd,
        session_dir,
        session_id.map(|id| NewSessionOptions {
            id: Some(id.to_owned()),
            ..Default::default()
        }),
        None,
    )
    .map_err(|error| {
        console_error(&Chalk::stderr().red(&format!("Error: {error}")));
        MainExit(1)
    })
}

// =============================================================================
// Session manager creation
// =============================================================================

/// `createSessionManager`.
pub async fn create_session_manager(
    parsed: &Args,
    cwd: &str,
    session_dir: Option<&str>,
    _settings_manager: &SettingsManager,
    control_db_path: Option<&str>,
) -> MainResult<SessionManager> {
    if parsed.no_session.is_some_and(|value| value)
        || parsed.help.is_some_and(|value| value)
        || parsed.list_models.is_some()
        || parsed.list_tools.is_some_and(|value| value)
        || parsed.list_extensions.is_some_and(|value| value)
    {
        let options = parsed.session_id.as_ref().map(|id| NewSessionOptions {
            id: Some(id.clone()),
            ..Default::default()
        });
        return SessionManager::in_memory(Some(cwd), options).map_err(|error| {
            console_error(&Chalk::stderr().red(&format!("Error: {error}")));
            MainExit(1)
        });
    }

    if let Some(fork) = parsed.fork.as_deref() {
        if let Some(session_id) = parsed.session_id.as_deref()
            && let Ok(Some(ResolvedSession::Local(_))) =
                find_local_session_by_exact_id(session_id, cwd, session_dir, control_db_path)
        {
            console_error(&Chalk::stderr().red(&format!("Session already exists with id '{session_id}'")));
            return Err(MainExit(1));
        }

        let resolved = resolve_session_path(fork, cwd, session_dir, control_db_path).map_err(|error| {
            console_error(&Chalk::stderr().red(&format!("Error: {error}")));
            MainExit(1)
        })?;

        return match resolved {
            ResolvedSession::Path(path) | ResolvedSession::Local(path) | ResolvedSession::Global { path, .. } => {
                fork_session_or_exit(&path, cwd, session_dir, parsed.session_id.as_deref())
            }
            ResolvedSession::NotFound(arg) => {
                console_error(&Chalk::stderr().red(&format!("No session found matching '{arg}'")));
                Err(MainExit(1))
            }
        };
    }

    if let Some(session) = parsed.session.as_deref() {
        let resolved = resolve_session_path(session, cwd, session_dir, control_db_path).map_err(|error| {
            console_error(&Chalk::stderr().red(&format!("Error: {error}")));
            MainExit(1)
        })?;

        return match resolved {
            ResolvedSession::Path(path) | ResolvedSession::Local(path) => open_session_or_exit(&path, session_dir),
            ResolvedSession::Global { path, cwd: global_cwd } => {
                console_log(&Chalk::stdout().yellow(&format!("Session found in different project: {global_cwd}")));
                let should_fork = prompt_confirm("Fork this session into current directory?").await;
                if !should_fork {
                    console_log(&Chalk::stdout().dim("Aborted."));
                    return Err(MainExit(0));
                }
                fork_session_or_exit(&path, cwd, session_dir, None)
            }
            ResolvedSession::NotFound(arg) => {
                console_error(&Chalk::stderr().red(&format!("No session found matching '{arg}'")));
                Err(MainExit(1))
            }
        };
    }

    if parsed.resume.is_some_and(|value| value) {
        console_error(&Chalk::stderr().red(
            "Error: --resume requires the interactive session picker, which is not ported (cli/session-picker.ts selectSession)",
        ));
        return Err(MainExit(1));
    }

    if parsed.continue_session.is_some_and(|value| value) {
        return SessionManager::continue_recent(cwd, session_dir).map_err(|error| {
            console_error(&Chalk::stderr().red(&format!("Error: {error}")));
            MainExit(1)
        });
    }

    if let Some(session_id) = parsed.session_id.as_deref()
        && let Ok(Some(ResolvedSession::Local(path))) =
            find_local_session_by_exact_id(session_id, cwd, session_dir, control_db_path)
    {
        return open_session_or_exit(&path, session_dir);
    }
    if let Some(session_id) = parsed.session_id.as_deref() {
        console_error(&Chalk::stderr().yellow(&format!(
            "Warning: No project session found with id '{session_id}'; creating a new session with that id."
        )));
    }

    let options = parsed.session_id.as_ref().map(|id| NewSessionOptions {
        id: Some(id.clone()),
        ..Default::default()
    });
    SessionManager::create(cwd, session_dir, options).map_err(|error| {
        console_error(&Chalk::stderr().red(&format!("Error: {error}")));
        MainExit(1)
    })
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
    model_runtime: &ModelRuntime,
    settings_manager: &SettingsManager,
) -> SessionOptionsBuild {
    let mut options = CreateAgentSessionOptions::default();
    let mut diagnostics = Vec::new();
    let mut cli_thinking_from_model = false;

    if let Some(cli_model) = parsed.model.as_deref() {
        let resolved = resolve_cli_model(
            &ResolveCliModelOptions {
                cli_provider: parsed.provider.clone(),
                cli_model: Some(cli_model.to_owned()),
                cli_thinking: parsed.thinking,
            },
            model_runtime,
        );
        if let Some(warning) = resolved.warning {
            diagnostics.push(AgentSessionRuntimeDiagnostic {
                diagnostic_type: "warning".to_owned(),
                message: warning,
            });
        }
        if let Some(error) = resolved.error {
            diagnostics.push(AgentSessionRuntimeDiagnostic {
                diagnostic_type: "error".to_owned(),
                message: error,
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
            (Some(provider), Some(model_id)) => model_runtime.get_model(provider, model_id),
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

    SessionOptionsBuild {
        options,
        cli_thinking_from_model,
        diagnostics,
    }
}

/// `resolveCliPaths`.
pub fn resolve_cli_paths(cwd: &str, paths: Option<&[String]>) -> Result<Option<Vec<String>>, String> {
    paths
        .map(|paths| {
            paths
                .iter()
                .map(|value| {
                    if is_local_path(value) {
                        resolve_path(value, cwd, &PathInputOptions::default()).map_err(|error| error.to_string())
                    } else {
                        Ok(value.clone())
                    }
                })
                .collect()
        })
        .transpose()
}

/// `normalizePath` for the `--session-dir` argument.
pub fn normalize_session_dir(value: &str) -> Result<String, String> {
    normalize_path(value, &PathInputOptions::default()).map_err(|error| error.to_string())
}

/// `getAgentDir` as a string.
pub fn agent_dir_string() -> String {
    get_agent_dir().display().to_string()
}

/// Default session directory for a cwd (`getDefaultSessionDir`).
pub fn default_session_dir(cwd: &str, agent_dir: Option<&str>) -> Result<String, String> {
    get_default_session_dir(cwd, agent_dir).map_err(|error| error.to_string())
}

/// `ListModels` search text (`None` means all models).
pub fn list_models_search(parsed: &Args) -> Option<Option<&str>> {
    match parsed.list_models.as_ref()? {
        ListModels::All => Some(None),
        ListModels::Search(pattern) => Some(Some(pattern.as_str())),
    }
}

/// `AuthOperationOptions` carrying a 15 second abort timeout.
pub fn auth_operation_options(signal: CancellationToken) -> AuthOperationOptions {
    AuthOperationOptions::new(Some(signal))
}

// =============================================================================
// CLI orchestration
// =============================================================================

struct CliControlDependencies;
impl ControlCommandDependencies for CliControlDependencies {}

struct CliSessionsDependencies;
impl SessionsCommandDependencies for CliSessionsDependencies {}

struct CliDebugDependencies {
    agent_dir: String,
}

impl DebugCommandDependencies for CliDebugDependencies {
    fn agent_dir(&self) -> String {
        self.agent_dir.clone()
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
        .flat_map(|extension| extension.flags.values())
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

async fn read_piped_stdin() -> Option<String> {
    if std::io::stdin().is_terminal() {
        return None;
    }
    use tokio::io::AsyncReadExt;
    let mut data = String::new();
    let mut stdin = tokio::io::stdin();
    if stdin.read_to_string(&mut data).await.is_err() {
        return None;
    }
    let trimmed = data.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

async fn prompt_for_missing_session_cwd(
    issue: &crate::core::session_cwd::SessionCwdIssue,
    settings_manager: &SettingsManager,
) -> Option<String> {
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let terminal = Rc::new(RefCell::new(pi_tui::terminal::ProcessTerminal::new()));
    let options = vec![
        ("Continue".to_owned(), Some(issue.fallback_cwd.clone())),
        ("Cancel".to_owned(), None),
    ];
    crate::cli::startup_ui::show_startup_selector(
        settings_manager,
        terminal,
        &env,
        &crate::core::session_cwd::format_missing_session_cwd_prompt(issue),
        options,
    )
    .await
    .ok()
    .flatten()
    .flatten()
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
pub async fn main(args: &[String], options: MainOptions) -> Result<i32, String> {
    let mut exit_code = 0;
    match run_main_inner(args, options, &mut exit_code).await {
        Ok(Some(code)) => Ok(code),
        Ok(None) => Ok(exit_code),
        Err(message) => Err(message),
    }
}

/// Rust entry glue: `runCliActionOrReportError(() => main(...))` with the
/// explicit exit code preserved.
pub async fn run_main_or_report_error(args: &[String], options: MainOptions) -> i32 {
    match main(args, options).await {
        Ok(code) => code,
        Err(message) => {
            console_error(&Chalk::stderr().red(&format!("Error: {message}")));
            1
        }
    }
}

async fn run_main_inner(args: &[String], options: MainOptions, exit_code: &mut i32) -> Result<Option<i32>, String> {
    match parse_resident_console_args(args) {
        Ok(Some(command)) => {
            return Err(format!(
                "--{} resident console is not ported (cli/resident-console-command.ts)",
                command.service.as_str()
            ));
        }
        Ok(None) => {}
        Err(message) => return Err(message),
    }
    if args.len() == 1 && args[0] == "architect" {
        crate::architect::run_architect_service().await?;
        return Ok(None);
    }
    if args.len() == 1 && args[0] == "supervisor" {
        crate::supervisor::run_supervisor_service().await?;
        return Ok(None);
    }

    reset_timings(TimingLabel::Main);
    let package_command_extension_factories = options.extension_factories.clone();
    let offline_mode = offline_mode_from_env(args);
    if offline_mode {
        // `process.env.PI_OFFLINE = "1"` and `PI_SKIP_VERSION_CHECK = "1"` cannot
        // be applied safely in edition 2024; see the traceability deviations.
    }

    if run_auth_command(args, exit_code).await {
        return Ok(None);
    }

    if cfg!(windows) {
        cleanup_windows_self_update_quarantine(&get_package_dir());
    }

    let mut cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let agent_dir = agent_dir_string();
    let _bootstrap_settings_manager = SettingsManager::create(
        &cwd,
        Some(Path::new(&agent_dir)),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    )
    .map_err(|error| error.to_string())?;
    let fetch = configure_http_dispatcher(crate::core::http_dispatcher::DEFAULT_HTTP_IDLE_TIMEOUT_MS as f64)?;

    let package_effects = DefaultPackageCommandEffects {
        cwd: cwd.clone(),
        agent_dir: agent_dir.clone(),
        install_context: InstallContext::for_process(),
        entrypoint: std::env::current_exe().ok().map(|path| path.display().to_string()),
        extensions_loader: Arc::new(DefaultProjectTrustExtensionsLoader::default()),
        config_selector: Rc::new(TuiConfigSelector {
            terminal: Rc::new(RefCell::new(pi_tui::terminal::ProcessTerminal::new())),
        }),
        fetch: Arc::clone(&fetch),
        extension_factories: package_command_extension_factories.clone(),
    };

    if let Some(outcome) = execute_package_command(args, &package_effects).await? {
        print_command_outcome(&outcome);
        let code = outcome.exit_code.unwrap_or(0);
        return Ok(Some(code));
    }
    if let Some(outcome) = execute_config_command(args, &package_effects).await? {
        print_command_outcome(&outcome);
        return Ok(None);
    }

    if run_auth_command(args, exit_code).await {
        return Ok(None);
    }

    let login = handle_login_command(
        args,
        LoginCommandDependencies {
            model_runtime: None,
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
    .await;
    if debug.handled {
        return Ok(Some(debug.exit_code.unwrap_or(0)));
    }

    let sessions = handle_sessions_command(args, &CliSessionsDependencies).await?;
    if sessions.handled {
        return Ok(Some(sessions.exit_code.unwrap_or(0)));
    }

    let mut parsed = parse_args(args);
    let mut env: BTreeMap<String, String> = std::env::vars().collect();
    let handoff = consume_self_restart_request(&mut env, std::process::id(), parent_process_id());
    apply_self_restart_request(&mut parsed, handoff.as_ref());
    wait_for_self_restart_parent_exit(handoff.as_ref(), &SelfRestartWaitDependencies::default()).await;
    if handoff.as_ref().and_then(|handoff| handoff.old_pid) == Some(std::process::id()) {
        prepare_control_db_for_self_restart(&get_control_db_path(&get_user_state_root()), std::process::id())
            .map_err(|error| error.to_string())?;
    }

    let multi_agent_store = create_first_party_multi_agent_store();
    let multi_agent_runtime_handles =
        crate::bundled_extensions::agents_core::runtime::create_multi_agent_runtime_handles();
    let first_party_extension_factories =
        create_first_party_extension_factories(Arc::clone(&multi_agent_store), multi_agent_runtime_handles.clone());
    let extension_factories = if parsed.no_extensions.is_some_and(|value| value) {
        options.extension_factories.clone()
    } else {
        let mut factories = first_party_extension_factories;
        factories.extend(options.extension_factories.clone());
        factories
    };

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
    time("parseArgs", TimingLabel::Main);

    if parsed.version.is_some_and(|value| value) {
        console_log(&package_config().version);
        return Ok(Some(0));
    }

    if let Some(export_path) = parsed.export.as_deref() {
        let output_path = parsed.messages.first().cloned();
        let options = output_path.map(ExportOptions::from_output_path);
        let result = export_from_file(export_path, options)
            .await
            .map_err(|error| error.message().to_owned())?;
        console_log(&format!("Exported to: {result}"));
        return Ok(Some(0));
    }

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
        cwd = resolve_worktree(worktree, Path::new(&cwd))
            .map(|path| path.display().to_string())
            .map_err(|error| error.to_string())?;
    }

    let migrations = run_migrations(&cwd);
    time("runMigrations", TimingLabel::Main);

    let mut startup_settings_manager = SettingsManager::create(
        &cwd,
        Some(Path::new(&agent_dir)),
        SettingsManagerCreateOptions { project_trusted: None },
    )
    .map_err(|error| error.to_string())?;
    report_diagnostics(&collect_settings_diagnostics(
        &mut startup_settings_manager,
        "startup session lookup",
    ));

    if app_mode == AppMode::Interactive
        && parsed.help != Some(true)
        && parsed.list_models.is_none()
        && crate::cli::startup_ui::should_run_first_time_setup_for_process()
    {
        return Err("first-time setup is not ported (cli/startup-ui.ts showFirstTimeSetup)".to_owned());
    }

    let env_session_dir = std::env::var(crate::config::env_session_dir()).ok();
    let session_dir = match parsed.session_dir.as_deref() {
        Some(dir) => Some(normalize_session_dir(dir)?),
        None => match env_session_dir {
            Some(dir) => Some(crate::config::expand_tilde_path(&dir)),
            None => startup_settings_manager.get_session_dir(),
        },
    };
    let control_db_path = get_control_db_path(&get_user_state_root());
    let mut session_manager = match create_session_manager(
        &parsed,
        &cwd,
        session_dir.as_deref(),
        &startup_settings_manager,
        Some(&control_db_path),
    )
    .await
    {
        Ok(manager) => manager,
        Err(MainExit(code)) => return Ok(Some(code)),
    };
    session_manager
        .set_metadata_control_db_path(
            Some(&control_db_path),
            crate::core::session_manager::SetMetadataControlDbPathOptions {
                index_message_text: None,
                cwd_override: None,
            },
        )
        .map_err(|error| error.to_string())?;
    if let Some(issue) = get_missing_session_cwd_issue(&session_manager, &cwd) {
        if handoff.is_some() {
            session_manager =
                reopen_session_with_cwd(&issue, session_dir.as_deref(), &issue.fallback_cwd, &control_db_path)
                    .map_err(|error| error.to_string())?;
        } else if app_mode == AppMode::Interactive {
            let Some(selected_cwd) = prompt_for_missing_session_cwd(&issue, &startup_settings_manager).await else {
                return Ok(Some(0));
            };
            session_manager = reopen_session_with_cwd(&issue, session_dir.as_deref(), &selected_cwd, &control_db_path)
                .map_err(|error| error.to_string())?;
        } else {
            console_error(&Chalk::stderr().red(&MissingSessionCwdError::new(issue).to_string()));
            return Ok(Some(1));
        }
    }
    append_self_restart_notice(&mut session_manager, handoff.as_ref(), std::process::id())
        .map_err(|error| error.to_string())?;
    if let Some(name) = parsed.name.as_deref() {
        let name = name.trim();
        if name.is_empty() {
            console_error(&Chalk::stderr().red("Error: --name requires a non-empty value"));
            return Ok(Some(1));
        }
        session_manager
            .append_session_info(name)
            .map_err(|error| error.to_string())?;
    }
    if should_take_over_stdout {
        take_over_stdout();
    }
    time("createSessionManager", TimingLabel::Main);
    if app_mode == AppMode::Interactive {
        let _control_message = claim_latest_incoming_message(&control_db_path).map_err(|error| error.to_string())?;
    }

    let trust_store = Arc::new(ProjectTrustStore::new(&agent_dir));
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
        multi_agent_store: Arc::clone(&multi_agent_store),
        multi_agent_runtime_handles: multi_agent_runtime_handles.clone(),
    });
    time("createRuntime", TimingLabel::Main);
    let session_cwd = session_manager.get_cwd().to_owned();
    let runtime = create_agent_session_runtime(create_runtime, &session_cwd, &agent_dir, session_manager).await?;
    reconcile_session_runtime_bindings(&control_db_path, &SessionDirectoryOptions::default())
        .map_err(|error| error.to_string())?;
    LifecycleCoordinator::reconcile_dead_detached_runtimes(&control_db_path, &now_iso_string())
        .map_err(|error| error.to_string())?;
    time("createAgentSessionRuntime", TimingLabel::Main);

    let services = runtime.services();
    let settings_manager = Arc::clone(&services.settings_manager);
    let model_runtime = services.model_runtime.clone();
    let resource_loader = Arc::clone(&services.resource_loader);
    let session = runtime.session().clone();
    let model_fallback_message = runtime.model_fallback_message().map(str::to_owned);
    let (image_auto_resize, theme, http_idle_timeout_ms) = {
        let settings = settings_manager.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            settings.get_image_auto_resize(),
            settings.get_theme().map(str::to_owned),
            settings.get_http_idle_timeout_ms().map_err(|error| error.to_string())?,
        )
    };
    let _ = configure_http_dispatcher(http_idle_timeout_ms as f64)?;

    if parsed.help == Some(true) {
        write_stdout(&print_help(&extension_flags_from_loader(&resource_loader)));
        return Ok(Some(0));
    }

    if let Some(search_pattern) = list_models_search(&parsed) {
        let timeout = TimeoutSignal::new(15_000);
        let output = list_models(&model_runtime, search_pattern, Some(timeout.token())).await?;
        if let Some(warning) = output.warning {
            console_error(&warning);
        }
        write_stdout(&output.stdout);
        return Ok(Some(0));
    }

    if parsed.list_tools == Some(true) || parsed.list_extensions == Some(true) {
        session.bind_extensions(ExtensionBindings::default()).await;
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
        let content = read_piped_stdin().await;
        if content.is_some() && app_mode == AppMode::Interactive {
            app_mode = AppMode::Print;
        }
        content
    } else {
        None
    };
    time("readPipedStdin", TimingLabel::Main);

    let initial = prepare_initial_message(&mut parsed, image_auto_resize, stdin_content.as_deref()).await?;
    time("prepareInitialMessage", TimingLabel::Main);
    init_theme(theme.as_deref(), app_mode == AppMode::Interactive).map_err(|error| error.to_string())?;
    time("initTheme", TimingLabel::Main);

    if app_mode == AppMode::Interactive && !migrations.deprecation_warnings.is_empty() {
        show_deprecation_warnings(&migrations.deprecation_warnings).await;
    }
    time("resolveModelScope", TimingLabel::Main);
    report_diagnostics(runtime.diagnostics());
    if runtime
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.diagnostic_type == "error")
    {
        if runtime
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Failed to load extension"))
        {
            console_error(&Chalk::stderr().yellow("Hint: Start without extensions using \"pi -ne\"."));
        }
        return Ok(Some(1));
    }
    time("createAgentSession", TimingLabel::Main);

    let no_models_message = format_no_models_available_message();
    if app_mode != AppMode::Interactive && model_fallback_message.as_deref() == Some(no_models_message.as_str()) {
        console_error(&Chalk::stderr().red(&no_models_message));
        return Ok(Some(1));
    }

    let startup_benchmark = is_truthy_env_flag(std::env::var("PI_STARTUP_BENCHMARK").ok().as_deref());
    if startup_benchmark && app_mode != AppMode::Interactive {
        console_error(&Chalk::stderr().red("Error: PI_STARTUP_BENCHMARK only supports interactive mode"));
        return Ok(Some(1));
    }

    refresh_rpc_model_catalogs(&model_runtime, offline_mode, app_mode);

    if app_mode == AppMode::Rpc {
        print_timings();
        run_rpc_mode(runtime).await;
        return Ok(None);
    }
    if app_mode == AppMode::Interactive {
        return Err("Interactive mode is not ported (modes/interactive/interactive-mode.ts)".to_owned());
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
            messages: parsed.messages.clone(),
            initial_message: initial.initial_message,
            initial_images: initial.initial_images.unwrap_or_default(),
        },
    )
    .await;
    stop_theme_watcher();
    restore_stdout();
    Ok((result != 0).then_some(result))
}
