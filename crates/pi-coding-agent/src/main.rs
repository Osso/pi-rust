//! Port of `packages/coding-agent/src/main.ts`.
//!
//! This module mirrors the startup helpers of the TypeScript entry point.
//! The orchestration (`main`) is added once the in-flight mode/runtime slices
//! land on `master`; see `docs/porting/traceability/main.md` for the current
//! mapping and the explicit gaps.

use std::future::Future;
use std::io::Write;
use std::path::Path;

use pi_ai::auth::types::{AuthOperationOptions, CredentialStore};
use pi_ai::models::models_are_equal;
use tokio_util::sync::CancellationToken;

use crate::cli::args::{Args, ListModels, Mode, parse_args};
use crate::cli::auth_check::{
    AuthCheckReason, AuthCheckResult, AuthCheckStatus, CheckProviderAuthOptions, check_provider_auth,
    create_auth_check_model_runtime, get_provider_credential,
};
use crate::cli::credential_print::{CredentialPrintKind, resolve_credential_for_print};
use crate::cli::file_processor::{ProcessFileOptions, process_file_arguments};
use crate::cli::initial_message::{InitialMessageInput, InitialMessageResult, build_initial_message};
use crate::cli::{
    AuthCommand, AuthCommandError, AuthCommandKind, AuthCommandTarget, get_auth_command_name, get_auth_command_usage,
    is_auth_command_help, parse_auth_command, print_auth_command_help, validate_auth_command_args,
};
use crate::config::{get_agent_dir, package_config};
use crate::core::agent_session_services::AgentSessionRuntimeDiagnostic;
use crate::core::auth_storage::{AuthStorage, ReadOnlyAuthStorage};
use crate::core::external_session_importer::{
    ExternalSessionImportOptions, import_external_session_alias, is_external_session_alias,
};
use crate::core::model_resolver::{ResolveCliModelOptions, ScopedModel, resolve_cli_model};
use crate::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use crate::core::project_trust::AppMode;
use crate::core::sdk::{CreateAgentSessionOptions, NoTools};
use crate::core::session_manager::{
    NewSessionOptions, SessionInfo, SessionManager, assert_valid_session_id, get_default_session_dir,
};
use crate::core::settings_manager::SettingsManager;
use crate::utils::chalk::Chalk;
use crate::utils::git_project::read_git_common_directory;
use crate::utils::paths::{PathInputOptions, is_local_path, normalize_path, resolve_path};

/// `process.exit(code)` represented as an error so helpers can stop the flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MainExit(pub i32);

/// Result of a helper that may call `process.exit`.
pub type MainResult<T> = Result<T, MainExit>;

/// `MainOptions`.
#[derive(Default)]
pub struct MainOptions {
    /// Inline extension factories appended after the first-party factories.
    pub extension_factories: Vec<crate::core::extensions::loader::ExtensionFactory>,
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
        console_log(&print_auth_command_help());
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
