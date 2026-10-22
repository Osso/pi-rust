//! Ported from `packages/coding-agent/src/core/tools/bash.ts`.
//!
//! Local shell execution with streaming output, throttled updates, timeout and
//! abort mapping, tail truncation with a full-output spill file, and the
//! runner-owned detach path backed by the detached job lifecycle controller.

use crate::utils::paths::{node_is_absolute, node_join};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsValue;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject as Map, JsString, JsValue as Value, encode_node_utf8};
use pi_js_compat::number::js_number_to_string;
use pi_tui::Text;
use pi_tui::tui::{Component, ComponentRef, Container};
use pi_tui::utils::truncate_to_width_with;
use serde_json::Value as SchemaValue;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use super::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use super::render_utils::{get_text_output, invalid_arg_text, str_value};
use super::tool_definition_wrapper::create_tool_definition_from_agent_tool;
use super::tool_definition_wrapper::wrap_agent_tool;
use super::truncate::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncatedBy, TruncationResult, format_size};
use crate::core::detached_bash_runner::{
    DetachedBashForegroundCompletion, is_process_identity_alive, terminate_foreground_runner,
    write_detached_bash_activation,
};
use crate::core::detached_job_runner::{
    DetachedJobAgentType, DetachedJobLifecycleController, DetachedJobOwnership, LaunchDetachedBashInput,
    RegisterDetachedJobInput,
};
use crate::core::extensions::types::{
    ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_call, infallible_render_result,
};
use crate::core::multi_agent_store::{AgentLifecycle, is_active_lifecycle};
use crate::core::tool_detach_registry::{ToolDetachHandle, ToolDetachRegistry};
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::components::visual_truncate::truncate_to_visual_lines;
use crate::modes::interactive::theme::{Theme, ThemeColor, current_theme};
use crate::utils::child_process::node_error_code;
use crate::utils::shell::{
    CommandTransport, get_shell_config, get_shell_env, kill_process_tree, track_detached_child_pid,
    untrack_detached_child_pid,
};

const MAX_TIMEOUT_MS: f64 = 2_147_483_647.0;
const MAX_TIMEOUT_SECONDS: f64 = MAX_TIMEOUT_MS / 1000.0;
const BASH_UPDATE_THROTTLE_MS: u64 = 100;
const DESCRIPTION_PREFIX: &str =
    "Execute a bash command in the current working directory. Returns stdout and stderr. Output is truncated to last";

/// `bashToolSystemPromptContribution`.
pub const BASH_TOOL_SYSTEM_PROMPT_CONTRIBUTION: super::ToolSystemPromptContribution =
    super::ToolSystemPromptContribution {
        snippet: "Execute bash commands (ls, grep, find, etc.)",
        guidelines: &[],
    };

/// `BashToolDetachRegistry`: the shared detach registry for bash tool calls.
pub type BashToolDetachRegistry = ToolDetachRegistry;

/// Output artifact paths for a detached job.
pub use crate::core::detached_job_runner::DetachedJobArtifacts;

/// A command that moved to the background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashDetachedResult {
    pub job_id: JsString,
    pub message: JsString,
    pub log_path: Option<JsString>,
}

/// Detach request passed to [`BashOperations::exec`].
#[derive(Clone)]
pub struct BashDetachOptions {
    pub artifacts: Option<DetachedJobArtifacts>,
    pub job_id: Option<JsString>,
    pub signal: CancellationToken,
}

/// `BashSpawnContext`: command, cwd, and environment before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashSpawnContext {
    pub command: JsString,
    pub cwd: JsString,
    pub env: BTreeMap<JsString, JsString>,
}

/// `BashSpawnHook`.
pub type BashSpawnHook = Arc<dyn Fn(BashSpawnContext) -> BashSpawnContext + Send + Sync>;

/// Session metadata exposed as `PI_*` environment variables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BashSessionEnvironment {
    pub session_id: JsString,
    pub session_file: Option<JsString>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub reasoning_level: Option<String>,
}

/// Reads the current session environment at execution time.
pub type BashSessionEnvironmentProvider = Arc<dyn Fn() -> Option<BashSessionEnvironment> + Send + Sync>;

/// `BashBackgroundJobsOptions`: resolves the session's detached job lifecycle.
#[derive(Clone)]
pub struct BashBackgroundJobs {
    pub get_lifecycle: Arc<dyn Fn() -> Option<Arc<DetachedJobLifecycleController>> + Send + Sync>,
    /// Fixed lifecycle used when `get_lifecycle` yields nothing.
    pub lifecycle: Option<Arc<DetachedJobLifecycleController>>,
}

/// Result of executing one shell command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashExecResult {
    pub exit_code: Option<i32>,
    pub detached: Option<BashDetachedResult>,
}

/// Streaming output callback.
pub type BashDataCallback = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// Data and lifecycle hooks for one shell execution.
pub struct BashExecOptions {
    pub on_data: BashDataCallback,
    pub signal: Option<CancellationToken>,
    pub timeout: Option<f64>,
    pub env: BTreeMap<JsString, JsString>,
    pub detach: Option<BashDetachOptions>,
}

/// Pluggable shell execution. Override to delegate commands to a remote system.
pub trait BashOperations: Send + Sync {
    fn exec<'a>(
        &'a self,
        command: &'a JsString,
        cwd: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>>;
}

/// Local shell execution.
#[derive(Debug, Default, Clone)]
pub struct LocalBashOperations {
    shell_path: Option<JsString>,
}

impl LocalBashOperations {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_shell_path(shell_path: impl Into<JsString>) -> Self {
        Self {
            shell_path: Some(shell_path.into()),
        }
    }
}

/// Resolves the shell, validates the working directory, and runs the command.
pub fn create_local_bash_operations(shell_path: Option<&JsString>) -> LocalBashOperations {
    match shell_path {
        Some(shell_path) => LocalBashOperations::with_shell_path(shell_path),
        None => LocalBashOperations::new(),
    }
}

fn resolve_timeout_ms(timeout: Option<f64>) -> Result<Option<f64>, ToolError> {
    let Some(timeout) = timeout else {
        return Ok(None);
    };
    if !timeout.is_finite() || timeout <= 0.0 {
        return Err(ToolError::new("Invalid timeout: must be a finite number of seconds"));
    }
    let timeout_ms = timeout * 1000.0;
    if timeout_ms > MAX_TIMEOUT_MS {
        return Err(ToolError::new(format!(
            "Invalid timeout: maximum is {} seconds",
            js_number_to_string(MAX_TIMEOUT_SECONDS)
        )));
    }
    Ok(Some(timeout_ms))
}

impl BashOperations for LocalBashOperations {
    fn exec<'a>(
        &'a self,
        command: &'a JsString,
        cwd: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move { exec_local(self.shell_path.as_ref(), command, cwd, options).await })
    }
}

async fn exec_local(
    shell_path: Option<&JsString>,
    command: &JsString,
    cwd: &JsString,
    options: BashExecOptions,
) -> Result<BashExecResult, ToolError> {
    let timeout_ms = resolve_timeout_ms(options.timeout)?;
    if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ToolError::new("aborted"));
    }
    let shell_config = get_shell_config(shell_path).map_err(|error| ToolError::new(error.message().clone()))?;
    if tokio::fs::metadata(encode_node_utf8(cwd)).await.is_err() {
        return Err(ToolError::new(js_format!(
            "Working directory does not exist: {}\nCannot execute bash commands.",
            cwd
        )));
    }

    let command_from_stdin = shell_config.command_transport == CommandTransport::Stdin;
    let output_file = match &options.detach {
        Some(detach) => match &detach.artifacts {
            Some(artifacts) => Some(open_output_file(&artifacts.output_path)?),
            None => None,
        },
        None => None,
    };

    let mut command_builder =
        tokio::process::Command::new(encode_node_utf8(&resolve_shell_executable(&shell_config.shell, cwd)));
    command_builder.current_dir(encode_node_utf8(cwd));
    if command_from_stdin {
        command_builder.args(shell_config.args.iter().map(encode_node_utf8));
        command_builder.stdin(Stdio::piped());
    } else {
        command_builder.args(shell_config.args.iter().map(encode_node_utf8));
        command_builder.arg(encode_node_utf8(command));
        command_builder.stdin(Stdio::null());
    }
    command_builder.env_clear();
    command_builder.envs(
        options
            .env
            .iter()
            .map(|(name, value)| (encode_node_utf8(name), encode_node_utf8(value))),
    );
    command_builder.stdout(match &output_file {
        Some(file) => Stdio::from(file.try_clone().map_err(|error| ToolError::new(error.to_string()))?),
        None => Stdio::piped(),
    });
    command_builder.stderr(match &output_file {
        Some(file) => Stdio::from(file.try_clone().map_err(|error| ToolError::new(error.to_string()))?),
        None => Stdio::piped(),
    });
    #[cfg(unix)]
    command_builder.process_group(0);

    let mut child = command_builder
        .spawn()
        .map_err(|error| ToolError::new(js_format!("spawn {} {}", shell_config.shell, node_error_code(&error))))?;
    let child_pid = child.id();
    if let Some(pid) = child_pid {
        track_detached_child_pid(pid);
    }
    if command_from_stdin && let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(encode_node_utf8(command).as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    let mut readers = Vec::new();
    if output_file.is_none() {
        if let Some(stdout) = child.stdout.take() {
            readers.push(spawn_reader(stdout, Arc::clone(&options.on_data)));
        }
        if let Some(stderr) = child.stderr.take() {
            readers.push(spawn_reader(stderr, Arc::clone(&options.on_data)));
        }
    }
    let mut poller = output_file.as_ref().and_then(|_| {
        options.detach.as_ref().and_then(|detach| {
            detach
                .artifacts
                .as_ref()
                .map(|artifacts| spawn_output_poller(artifacts.output_path.clone(), Arc::clone(&options.on_data)))
        })
    });

    let mut timed_out = false;
    let mut detached: Option<BashDetachedResult> = None;
    let exit_status = {
        let wait = child.wait();
        tokio::pin!(wait);
        let deadline =
            timeout_ms.map(|timeout_ms| tokio::time::Instant::now() + Duration::from_secs_f64(timeout_ms / 1000.0));
        let detach_signal = options.detach.as_ref().map(|detach| &detach.signal);
        tokio::select! {
            status = &mut wait => Some(status),
            () = cancellation(options.signal.as_ref()) => {
                if let Some(pid) = child_pid {
                    kill_process_tree(pid);
                }
                let _ = (&mut wait).await;
                None
            }
            () = cancellation(detach_signal) => {
                if let Some(poller) = &poller {
                    poller.poll_once(&options.on_data);
                }
                // The source keeps the timeout handle alive after detaching, so a
                // backgrounded command is still killed when its timeout expires.
                if let (Some(deadline), Some(pid)) = (deadline, child_pid) {
                    tokio::spawn(async move {
                        tokio::time::sleep_until(deadline).await;
                        kill_process_tree(pid);
                    });
                }
                detached = Some(BashDetachedResult {
                    job_id: "background".into(),
                    message: "Background job started".into(),
                    log_path: None,
                });
                None
            }
            () = sleep_until(deadline) => {
                timed_out = true;
                if let Some(pid) = child_pid {
                    kill_process_tree(pid);
                }
                let _ = (&mut wait).await;
                None
            }
        }
    };

    if let Some(poller) = poller.take() {
        poller.task.abort();
    }
    if detached.is_some() {
        for reader in readers {
            reader.abort();
        }
        return Ok(BashExecResult {
            exit_code: None,
            detached,
        });
    }
    for reader in readers {
        let _ = tokio::time::timeout(Duration::from_millis(100), reader).await;
    }
    if let Some(pid) = child_pid {
        untrack_detached_child_pid(pid);
    }
    if let Some(artifacts) = options.detach.as_ref().and_then(|detach| detach.artifacts.as_ref()) {
        let _ = std::fs::remove_dir_all(encode_node_utf8(&artifacts.directory));
    }
    if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ToolError::new("aborted"));
    }
    if timed_out {
        return Err(ToolError::new(format!(
            "timeout:{}",
            js_number_to_string(options.timeout.unwrap_or_default())
        )));
    }
    let status = match exit_status {
        Some(Ok(status)) => status,
        Some(Err(error)) => return Err(ToolError::new(error.to_string())),
        None => return Err(ToolError::new("Command did not produce an exit status")),
    };
    Ok(BashExecResult {
        exit_code: status.code(),
        detached: None,
    })
}

fn resolve_shell_executable(shell: &JsString, cwd: &JsString) -> JsString {
    if node_is_absolute(shell) || (!shell.contains('/') && !shell.contains('\\')) {
        shell.clone()
    } else {
        node_join(cwd, shell)
    }
}

fn open_output_file(path: &JsString) -> Result<std::fs::File, ToolError> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(encode_node_utf8(path))
        .map_err(|error| ToolError::new(js_format!("open {} {}", path, node_error_code(&error))))
}

fn spawn_reader<R>(mut reader: R, on_data: BashDataCallback) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(read) => on_data(&buffer[..read]),
            }
        }
    })
}

fn spawn_output_poller(path: JsString, on_data: BashDataCallback) -> DurableOutputPoller {
    let offset = Arc::new(AtomicU64::new(0));
    let poll_offset = Arc::clone(&offset);
    let poll_path = path.clone();
    let task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(25)).await;
            poll_durable_output(&poll_path, &poll_offset, &on_data);
        }
    });
    DurableOutputPoller { path, offset, task }
}

struct DurableOutputPoller {
    path: JsString,
    offset: Arc<AtomicU64>,
    task: tokio::task::JoinHandle<()>,
}

impl DurableOutputPoller {
    fn poll_once(&self, on_data: &BashDataCallback) {
        poll_durable_output(&self.path, &self.offset, on_data);
    }
}

fn poll_durable_output(path: &JsString, offset: &AtomicU64, on_data: &BashDataCallback) {
    let Ok(bytes) = std::fs::read(encode_node_utf8(path)) else {
        return;
    };
    loop {
        let current = offset.load(Ordering::SeqCst);
        let length = bytes.len() as u64;
        if length <= current {
            return;
        }
        if offset
            .compare_exchange(current, length, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            on_data(&bytes[current as usize..]);
            return;
        }
    }
}

async fn cancellation(signal: Option<&CancellationToken>) {
    match signal {
        Some(signal) => signal.cancelled().await,
        None => std::future::pending().await,
    }
}

async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Options for [`BashTool`].
#[derive(Clone, Default)]
pub struct BashToolOptions {
    /// Custom shell execution. Default: local shell.
    pub operations: Option<Arc<dyn BashOperations>>,
    /// Prefix prepended to every command on its own line.
    pub command_prefix: Option<JsString>,
    /// Shell executable override for local execution.
    pub shell_path: Option<JsString>,
    /// Whether to expose `PI_*` session environment variables. Default: true.
    pub expose_session_environment: Option<bool>,
    /// Hook to adjust command, cwd, or env before execution.
    pub spawn_hook: Option<BashSpawnHook>,
    /// Current session metadata for `PI_*` variables.
    pub session_environment: Option<BashSessionEnvironmentProvider>,
    /// Signal used to detach an in-flight command into a background job.
    pub detach: Option<BashDetachOptions>,
    /// Registry used by interactive controls to detach in-flight tool calls.
    pub detach_registry: Option<Arc<ToolDetachRegistry>>,
    /// Multi-agent detached job lifecycle for runner-owned execution.
    pub background_jobs: Option<Arc<BashBackgroundJobs>>,
}

/// Executes shell commands.
pub struct BashTool {
    cwd: JsString,
    parameters: SchemaValue,
    operations: Arc<dyn BashOperations>,
    custom_operations: bool,
    command_prefix: Option<JsString>,
    shell_path: Option<JsString>,
    expose_session_environment: bool,
    spawn_hook: Option<BashSpawnHook>,
    session_environment: Option<BashSessionEnvironmentProvider>,
    detach: Option<BashDetachOptions>,
    detach_registry: Option<Arc<ToolDetachRegistry>>,
    background_jobs: Option<Arc<BashBackgroundJobs>>,
}

impl BashTool {
    pub fn new(cwd: impl Into<JsString>) -> Self {
        Self::with_options(cwd, BashToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<JsString>, options: BashToolOptions) -> Self {
        let custom_operations = options.operations.is_some();
        let shell_path = options.shell_path.clone();
        let operations = options
            .operations
            .unwrap_or_else(|| Arc::new(create_local_bash_operations(options.shell_path.as_ref())));
        Self {
            cwd: cwd.into(),
            parameters: bash_schema(),
            operations,
            custom_operations,
            command_prefix: options.command_prefix,
            shell_path,
            expose_session_environment: options.expose_session_environment.unwrap_or(true),
            spawn_hook: options.spawn_hook,
            session_environment: options.session_environment,
            detach: options.detach,
            detach_registry: options.detach_registry,
            background_jobs: options.background_jobs,
        }
    }
}

fn bash_schema() -> SchemaValue {
    serde_json::json!({
        "type": "object",
        "required": ["command"],
        "properties": {
            "command": {"type": "string", "description": "Bash command to execute"},
            "timeout": {"type": "number", "description": "Timeout in seconds (optional, no default timeout)"}
        }
    })
}

static DESCRIPTION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "{DESCRIPTION_PREFIX} {DEFAULT_MAX_LINES} lines or {}KB (whichever is hit first). If truncated, full output is saved to a temp file. Optionally provide a timeout in seconds.",
        DEFAULT_MAX_BYTES / 1024
    )
});

impl AgentTool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        DESCRIPTION.as_str()
    }

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> &str {
        "bash"
    }

    fn constrained_sampling(&self) -> Option<pi_ai::types::ConstrainedSampling> {
        crate::core::experimental::get_experimental_tool_constrained_sampling()
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: JsValue,
        signal: Option<CancellationToken>,
        on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let cwd = self.cwd.clone();
        let operations = Arc::clone(&self.operations);
        let custom_operations = self.custom_operations;
        let command_prefix = self.command_prefix.clone();
        let shell_path = self.shell_path.clone();
        let expose_session_environment = self.expose_session_environment;
        let spawn_hook = self.spawn_hook.clone();
        let session_environment = self.session_environment.clone();
        let configured_detach = self.detach.clone();
        let detach_registry = self.detach_registry.clone();
        let background_jobs = self.background_jobs.clone();
        Box::pin(async move {
            execute_bash(
                &cwd,
                BashExecutionOptions {
                    operations,
                    custom_operations,
                    command_prefix,
                    shell_path,
                    expose_session_environment,
                    spawn_hook,
                    session_environment,
                    configured_detach,
                    detach_registry,
                    background_jobs,
                },
                tool_call_id,
                params,
                signal,
                on_update,
            )
            .await
        })
    }
}

struct BashExecutionOptions {
    operations: Arc<dyn BashOperations>,
    custom_operations: bool,
    command_prefix: Option<JsString>,
    shell_path: Option<JsString>,
    expose_session_environment: bool,
    spawn_hook: Option<BashSpawnHook>,
    session_environment: Option<BashSessionEnvironmentProvider>,
    configured_detach: Option<BashDetachOptions>,
    detach_registry: Option<Arc<ToolDetachRegistry>>,
    background_jobs: Option<Arc<BashBackgroundJobs>>,
}

fn resolve_spawn_context(
    command: JsString,
    cwd: &JsString,
    spawn_hook: Option<&BashSpawnHook>,
    expose_session_environment: bool,
    session_environment: Option<&BashSessionEnvironmentProvider>,
) -> BashSpawnContext {
    let mut env = get_shell_env();
    for name in [
        "PI_SESSION_ID",
        "PI_SESSION_FILE",
        "PI_PROVIDER",
        "PI_MODEL",
        "PI_REASONING_LEVEL",
    ] {
        env.remove(&JsString::from(name));
    }
    if expose_session_environment && let Some(session) = session_environment.and_then(|provider| provider()) {
        env.insert("PI_SESSION_ID".into(), session.session_id);
        if let Some(session_file) = session.session_file {
            env.insert("PI_SESSION_FILE".into(), session_file);
        }
        if let (Some(provider), Some(model)) = (session.provider, session.model) {
            env.insert("PI_PROVIDER".into(), provider.into());
            env.insert("PI_MODEL".into(), model.into());
        }
        if let Some(reasoning_level) = session.reasoning_level {
            env.insert("PI_REASONING_LEVEL".into(), reasoning_level.into());
        }
    }
    let base = BashSpawnContext {
        command,
        cwd: cwd.to_owned(),
        env,
    };
    match spawn_hook {
        Some(spawn_hook) => spawn_hook(base),
        None => base,
    }
}

async fn execute_bash(
    cwd: &JsString,
    options: BashExecutionOptions,
    tool_call_id: String,
    params: JsValue,
    signal: Option<CancellationToken>,
    on_update: AgentToolUpdateCallback,
) -> Result<AgentToolResult, ToolError> {
    let command = params
        .get("command")
        .and_then(Value::as_string)
        .ok_or_else(|| ToolError::new("bash tool requires a string command"))?
        .to_owned();
    let timeout = params.get("timeout").and_then(Value::as_f64);
    let resolved_command = match &options.command_prefix {
        Some(prefix) => JsString::from(prefix) + "\n" + command,
        None => command,
    };
    let spawn_context = resolve_spawn_context(
        resolved_command,
        cwd,
        options.spawn_hook.as_ref(),
        options.expose_session_environment,
        options.session_environment.as_ref(),
    );
    let lifecycle = options
        .background_jobs
        .as_ref()
        .and_then(|jobs| (jobs.get_lifecycle)().or_else(|| jobs.lifecycle.clone()));
    let detach_controller =
        if options.configured_detach.is_none() && lifecycle.is_some() && options.detach_registry.is_some() {
            Some(CancellationToken::new())
        } else {
            None
        };
    let detach = options.configured_detach.clone().or_else(|| {
        detach_controller.as_ref().map(|signal| BashDetachOptions {
            artifacts: None,
            job_id: None,
            signal: signal.clone(),
        })
    });
    let detach_subscription = detach_controller.as_ref().and_then(|signal| {
        options
            .detach_registry
            .as_ref()
            .map(|registry| registry.register(Arc::new(BashDetachHandle { signal: signal.clone() })))
    });
    let operations = Arc::clone(&options.operations);
    let custom_operations = options.custom_operations;
    let shell_path = options.shell_path.clone();
    let env = spawn_context.env.clone();
    let command = spawn_context.command.clone();
    let output = Arc::new(Mutex::new(OutputAccumulator::new(OutputAccumulatorOptions {
        temp_file_prefix: "pi-bash".into(),
        ..OutputAccumulatorOptions::default()
    })));
    let throttle = Arc::new(UpdateThrottle::new(on_update));
    throttle.emit_initial();
    let accepting_output = Arc::new(AtomicBool::new(true));

    let on_data: BashDataCallback = {
        let output = Arc::clone(&output);
        let throttle = Arc::clone(&throttle);
        let accepting_output = Arc::clone(&accepting_output);
        Arc::new(move |data: &[u8]| {
            if !accepting_output.load(Ordering::SeqCst) {
                return;
            }
            output.lock().expect("output lock").append(data);
            throttle.schedule(&output);
        })
    };

    let use_detached_runner = !custom_operations && lifecycle.is_some() && detach.is_some();
    let result = if use_detached_runner {
        execute_runner_owned_bash(
            &command,
            cwd,
            RunnerOwnedOptions {
                detach: detach.clone().expect("detach options"),
                env: env.clone(),
                lifecycle: lifecycle.clone().expect("lifecycle"),
                on_data: Arc::clone(&on_data),
                shell_path: shell_path.clone(),
                signal: signal.clone(),
                timeout,
                tool_call_id: tool_call_id.clone(),
            },
        )
        .await
    } else {
        operations
            .exec(
                &command,
                cwd,
                BashExecOptions {
                    on_data: Arc::clone(&on_data),
                    signal: signal.clone(),
                    timeout,
                    env: env.clone(),
                    detach,
                },
            )
            .await
            .map(|result| BashRunnerResult {
                exit_code: result.exit_code,
                detached: result.detached,
            })
    };
    if let Some(subscription) = detach_subscription {
        subscription.unregister();
    }

    accepting_output.store(false, Ordering::SeqCst);
    let snapshot = finish_output(&output, &throttle)?;
    match result {
        Ok(result) if result.detached.is_some() => {
            let detached = result.detached.expect("detached result");
            Ok(format_detached_output(&output, snapshot, detached))
        }
        Ok(result) => {
            let (text, details) = format_output(&output, &snapshot, "(no output)");
            if result.exit_code.is_some_and(|code| code != 0) {
                return Err(ToolError::new(append_status(
                    &text,
                    &format!("Command exited with code {}", result.exit_code.unwrap_or_default()),
                )));
            }
            Ok(AgentToolResult {
                content: vec![TextOrImageContent::Text(TextContent::new(text))],
                details,
                is_error: None,
                terminate: None,
            })
        }
        Err(error) => {
            let (text, _) = format_output(&output, &snapshot, "");
            if error.message == "aborted" {
                return Err(ToolError::new(append_status(&text, "Command aborted")));
            }
            if let Some(timeout_secs) = error.message.strip_prefix("timeout:") {
                return Err(ToolError::new(append_status(
                    &text,
                    JsString::from("Command timed out after ") + timeout_secs + " seconds",
                )));
            }
            Err(error)
        }
    }
}

struct BashDetachHandle {
    signal: CancellationToken,
}

impl ToolDetachHandle for BashDetachHandle {
    fn detach(&self) -> bool {
        if self.signal.is_cancelled() {
            return false;
        }
        self.signal.cancel();
        true
    }
}

struct RunnerOwnedOptions {
    detach: BashDetachOptions,
    env: BTreeMap<JsString, JsString>,
    lifecycle: Arc<DetachedJobLifecycleController>,
    on_data: BashDataCallback,
    shell_path: Option<JsString>,
    signal: Option<CancellationToken>,
    timeout: Option<f64>,
    tool_call_id: String,
}

struct BashRunnerResult {
    exit_code: Option<i32>,
    detached: Option<BashDetachedResult>,
}

async fn execute_runner_owned_bash(
    command: &JsString,
    cwd: &JsString,
    options: RunnerOwnedOptions,
) -> Result<BashRunnerResult, ToolError> {
    let shell =
        get_shell_config(options.shell_path.as_ref()).map_err(|error| ToolError::new(error.message().clone()))?;
    if shell.command_transport == CommandTransport::Stdin {
        return Err(ToolError::new(
            "Detached Bash runner does not support stdin shell transport",
        ));
    }
    if let Some(restored) = wait_for_restored_bash_job(&options).await? {
        return Ok(restored);
    }
    let mut args = shell.args.clone();
    args.push(command.clone());
    let runner = options
        .lifecycle
        .launch_bash(&LaunchDetachedBashInput {
            args,
            command: shell.shell,
            cwd: cwd.to_owned(),
            env: options
                .env
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            timeout_ms: resolve_timeout_ms(options.timeout)?.map(|timeout_ms| timeout_ms as i64),
            tool_call_id: Some(options.tool_call_id.clone().into()),
        })
        .map_err(|error| ToolError::new(error.to_string()))?;
    observe_foreground_bash_runner(runner, cwd, &options).await
}

async fn wait_for_restored_bash_job(options: &RunnerOwnedOptions) -> Result<Option<BashRunnerResult>, ToolError> {
    let restored_job = options
        .lifecycle
        .find_bash_job_by_tool_call_id(&JsString::from(&options.tool_call_id))
        .map_err(|error| ToolError::new(error.to_string()))?;
    let Some(mut restored_state) = restored_job else {
        return Ok(None);
    };
    while is_active_lifecycle(restored_state.lifecycle) {
        restored_state = options
            .lifecycle
            .observe(&restored_state.id)
            .map_err(|error| ToolError::new(error.to_string()))?
            .unwrap_or(restored_state);
        if is_active_lifecycle(restored_state.lifecycle) {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
    if restored_state.lifecycle == AgentLifecycle::Failed
        && restored_state.error.as_ref().and_then(|error| error.code.as_deref()) == Some("lost_runtime")
    {
        return Ok(None);
    }
    let output_path = restored_state
        .result
        .as_ref()
        .and_then(|result| result.file_refs.as_ref())
        .and_then(|file_refs| {
            file_refs
                .iter()
                .find(|file_ref| file_ref.label.as_ref().is_some_and(|label| label == "Bash output"))
                .map(|file_ref| file_ref.path.clone())
        });
    if let Some(output_path) = output_path
        && let Ok(output) = std::fs::read(encode_node_utf8(&output_path))
    {
        (options.on_data)(&output);
    }
    result_from_terminal_bash_agent(&restored_state, false, options.timeout).map(Some)
}

fn result_from_terminal_bash_agent(
    agent: &crate::core::multi_agent_store::AgentSnapshot,
    aborted: bool,
    timeout: Option<f64>,
) -> Result<BashRunnerResult, ToolError> {
    if aborted || agent.lifecycle == AgentLifecycle::Aborted {
        return Err(ToolError::new("aborted"));
    }
    if agent.lifecycle == AgentLifecycle::Failed
        && agent
            .error
            .as_ref()
            .is_some_and(|error| error.message.contains("timed out"))
    {
        return Err(ToolError::new(format!(
            "timeout:{}",
            timeout.map(format_timeout_seconds).unwrap_or_default()
        )));
    }
    Ok(BashRunnerResult {
        exit_code: Some(if agent.lifecycle == AgentLifecycle::Completed {
            0
        } else {
            1
        }),
        detached: None,
    })
}

fn format_timeout_seconds(timeout: f64) -> String {
    js_number_to_string(timeout)
}

async fn observe_foreground_bash_runner(
    runner: crate::core::detached_job_runner::LaunchedDetachedBashJob,
    cwd: &JsString,
    options: &RunnerOwnedOptions,
) -> Result<BashRunnerResult, ToolError> {
    let mut output_offset = 0_u64;
    let mut aborted = false;
    loop {
        output_offset = forward_bash_runner_output(&runner, output_offset, &options.on_data);
        if let Some(completion) = settle_foreground_bash_runner(&runner, aborted, options.timeout)? {
            return Ok(completion);
        }
        if options.detach.signal.is_cancelled()
            && let Some(activation) = activate_detached_bash_runner(&runner, cwd, options)?
        {
            return Ok(activation.1);
        }
        if !aborted && options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
            aborted = true;
            terminate_foreground_bash_runner(runner.runner_pid);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn forward_bash_runner_output(
    runner: &crate::core::detached_job_runner::LaunchedDetachedBashJob,
    output_offset: u64,
    on_data: &BashDataCallback,
) -> u64 {
    let Ok(output) = std::fs::read(encode_node_utf8(&runner.artifacts.output_path)) else {
        return output_offset;
    };
    if output.len() as u64 <= output_offset {
        return output_offset;
    }
    on_data(&output[output_offset as usize..]);
    output.len() as u64
}

fn settle_foreground_bash_runner(
    runner: &crate::core::detached_job_runner::LaunchedDetachedBashJob,
    aborted: bool,
    timeout: Option<f64>,
) -> Result<Option<BashRunnerResult>, ToolError> {
    if let Some(completion) = read_foreground_bash_completion(&runner.foreground_completion_path)? {
        if completion.timed_out {
            return Err(ToolError::new(format!(
                "timeout:{}",
                timeout.map(format_timeout_seconds).unwrap_or_default()
            )));
        }
        if aborted || completion.cancel_reason.is_some() {
            return Err(ToolError::new("aborted"));
        }
        return Ok(Some(BashRunnerResult {
            exit_code: Some(completion.exit_code.unwrap_or(1)),
            detached: None,
        }));
    }
    if is_process_identity_alive(&runner.process_identity) {
        return Ok(None);
    }
    let runner_error_path = js_format!("{}.runner-error", runner.manifest_path);
    if let Ok(message) = std::fs::read_to_string(encode_node_utf8(&runner_error_path)) {
        return Err(ToolError::new(message.trim().to_owned()));
    }
    if aborted {
        return Err(ToolError::new("aborted"));
    }
    Err(ToolError::new("Detached Bash runner exited without producing a result"))
}

fn read_foreground_bash_completion(path: &JsString) -> Result<Option<DetachedBashForegroundCompletion>, ToolError> {
    if !Path::new(&encode_node_utf8(path)).exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(encode_node_utf8(path)).map_err(|error| ToolError::new(error.to_string()))?;
    let value = pi_js_compat::js_json::parse_lossless(&data).map_err(|error| ToolError::new(error.to_string()))?;
    pi_js_compat::js_json::from_value(&value)
        .map(Some)
        .map_err(|error| ToolError::new(error.to_string()))
}

fn activate_detached_bash_runner(
    runner: &crate::core::detached_job_runner::LaunchedDetachedBashJob,
    cwd: &JsString,
    options: &RunnerOwnedOptions,
) -> Result<Option<(DetachedJobOwnership, BashRunnerResult)>, ToolError> {
    if !is_process_identity_alive(&runner.process_identity) {
        return Ok(None);
    }
    let ownership = match options.lifecycle.register(&RegisterDetachedJobInput {
        agent_type: DetachedJobAgentType::Bash,
        cwd: cwd.to_owned(),
        display_name: "Bash command".into(),
        job_id: runner.job_id.clone(),
        process_identity: runner.process_identity.clone(),
        worker_handle_id: runner.runner_pid.to_string().into(),
        tool_call_id: Some(options.tool_call_id.clone().into()),
        detached: Some(true),
        file_refs: None,
    }) {
        Ok(ownership) => ownership,
        Err(error) => {
            terminate_foreground_bash_runner(runner.runner_pid);
            return Err(ToolError::new(error.to_string()));
        }
    };
    if let Err(error) = write_detached_bash_activation(&runner.activation_path, &ownership.identity) {
        terminate_foreground_bash_runner(runner.runner_pid);
        return Err(ToolError::new(error.to_string()));
    }
    Ok(Some((
        ownership,
        BashRunnerResult {
            exit_code: None,
            detached: Some(BashDetachedResult {
                job_id: runner.job_id.clone(),
                message: js_format!("Detached bash command as background job {}.", runner.job_id),
                log_path: Some(runner.artifacts.output_path.clone()),
            }),
        },
    )))
}

fn terminate_foreground_bash_runner(pid: u32) {
    let _ = terminate_foreground_runner(pid, rustix::process::Signal::TERM);
}

fn finish_output(
    output: &Arc<Mutex<OutputAccumulator>>,
    throttle: &Arc<UpdateThrottle>,
) -> Result<super::output_accumulator::OutputSnapshot, ToolError> {
    let mut accumulator = output.lock().expect("output lock");
    accumulator.finish();
    throttle.mark_finished();
    let snapshot = accumulator.snapshot(SnapshotOptions {
        persist_if_truncated: true,
    });
    if throttle.take_dirty() {
        throttle.emit_snapshot(&snapshot);
    }
    accumulator
        .close_temp_file()
        .map_err(|error| ToolError::new(super::node_fs::node_fs_error(&error, "close", &JsString::default())))?;
    Ok(snapshot)
}

fn format_output(
    output: &Arc<Mutex<OutputAccumulator>>,
    snapshot: &super::output_accumulator::OutputSnapshot,
    empty_text: &str,
) -> (JsString, Option<Value>) {
    let truncation = &snapshot.truncation;
    let mut text = if snapshot.content.is_empty() {
        JsString::from(empty_text)
    } else {
        snapshot.content.clone()
    };
    let mut details = Map::new();
    if truncation.truncated {
        details.insert(
            "truncation".into(),
            pi_js_compat::js_json::to_value(truncation).expect("truncation metadata must serialize"),
        );
        if let Some(path) = &snapshot.full_output_path {
            details.insert("fullOutputPath".into(), Value::String(path.clone()));
        }
        let start_line = truncation.total_lines - truncation.output_lines + 1;
        let end_line = truncation.total_lines;
        let path = snapshot.full_output_path.clone().unwrap_or_default();
        let notice = if truncation.last_line_partial {
            let last_bytes = output.lock().expect("output lock").last_line_bytes();
            js_format!(
                "\n\n[Showing last {} of line {} (line is {}). Full output: {}]",
                format_size(truncation.output_bytes as f64),
                end_line.to_string(),
                format_size(last_bytes as f64),
                path
            )
        } else if truncation.truncated_by == Some(TruncatedBy::Lines) {
            js_format!(
                "\n\n[Showing lines {}-{} of {}. Full output: {}]",
                start_line.to_string(),
                end_line.to_string(),
                truncation.total_lines.to_string(),
                path
            )
        } else {
            js_format!(
                "\n\n[Showing lines {}-{} of {} ({} limit). Full output: {}]",
                start_line.to_string(),
                end_line.to_string(),
                truncation.total_lines.to_string(),
                format_size(DEFAULT_MAX_BYTES as f64),
                path
            )
        };
        text.push_str(notice);
    }
    (text, (!details.is_empty()).then_some(Value::Object(details)))
}

fn append_status(text: impl Into<JsString>, status: impl Into<JsString>) -> JsString {
    let text = text.into();
    let status = status.into();
    if text.is_empty() {
        status
    } else {
        text + "\n\n" + status
    }
}

fn format_detached_output(
    output: &Arc<Mutex<OutputAccumulator>>,
    snapshot: super::output_accumulator::OutputSnapshot,
    detached: BashDetachedResult,
) -> AgentToolResult {
    let (text, _) = format_output(output, &snapshot, "");
    let text = text.trim_end();
    let mut details = Map::new();
    details.insert("backgroundJobId".into(), Value::String(detached.job_id.clone().into()));
    if let Some(log_path) = &detached.log_path {
        details.insert("fullOutputPath".into(), Value::String(log_path.clone().into()));
    }
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(append_status(
            &text,
            js_format!(
                "Command moved to background as job {}. {}",
                detached.job_id,
                detached.message
            ),
        )))],
        details: Some(Value::Object(details)),
        is_error: None,
        terminate: None,
    }
}

struct UpdateThrottle {
    on_update: AgentToolUpdateCallback,
    last_update_at: Mutex<Instant>,
    timer_running: AtomicBool,
    dirty: AtomicBool,
    finished: AtomicBool,
}

impl UpdateThrottle {
    fn new(on_update: AgentToolUpdateCallback) -> Self {
        Self {
            on_update,
            last_update_at: Mutex::new(
                Instant::now()
                    .checked_sub(Duration::from_millis(BASH_UPDATE_THROTTLE_MS))
                    .unwrap_or_else(Instant::now),
            ),
            timer_running: AtomicBool::new(false),
            dirty: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        }
    }

    fn emit_initial(&self) {
        (self.on_update)(AgentToolResult {
            content: Vec::new(),
            details: None,
            is_error: None,
            terminate: None,
        });
    }

    fn mark_finished(&self) {
        self.finished.store(true, Ordering::SeqCst);
    }

    fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::SeqCst)
    }

    fn schedule(self: &Arc<Self>, output: &Arc<Mutex<OutputAccumulator>>) {
        if self.finished.load(Ordering::SeqCst) {
            return;
        }
        self.dirty.store(true, Ordering::SeqCst);
        let elapsed = self.last_update_at.lock().expect("throttle lock").elapsed();
        if elapsed.as_millis() >= u128::from(BASH_UPDATE_THROTTLE_MS) {
            self.emit_now(output);
            return;
        }
        if self.timer_running.swap(true, Ordering::SeqCst) {
            return;
        }
        let throttle = Arc::clone(self);
        let output = Arc::clone(output);
        let delay = Duration::from_millis(BASH_UPDATE_THROTTLE_MS).saturating_sub(elapsed);
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            throttle.timer_running.store(false, Ordering::SeqCst);
            throttle.emit_now(&output);
        });
    }

    fn emit_now(&self, output: &Arc<Mutex<OutputAccumulator>>) {
        if self.finished.load(Ordering::SeqCst) {
            return;
        }
        self.dirty.store(false, Ordering::SeqCst);
        let snapshot = output.lock().expect("output lock").snapshot(SnapshotOptions {
            persist_if_truncated: true,
        });
        self.emit_snapshot(&snapshot);
        *self.last_update_at.lock().expect("throttle lock") = Instant::now();
    }

    fn emit_snapshot(&self, snapshot: &super::output_accumulator::OutputSnapshot) {
        let mut details = Map::new();
        if snapshot.truncation.truncated {
            details.insert(
                "truncation".into(),
                pi_js_compat::js_json::to_value(&snapshot.truncation).expect("truncation metadata must serialize"),
            );
        }
        if let Some(path) = &snapshot.full_output_path {
            details.insert("fullOutputPath".into(), Value::String(path.clone().into()));
        }
        (self.on_update)(AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(snapshot.content.clone()))],
            details: Some(Value::Object(details)),
            is_error: None,
            terminate: None,
        });
    }
}

// ============================================================================
// Renderers
// ============================================================================

fn format_bash_call(args: &JsValue) -> JsString {
    let theme = current_theme();
    let timeout = args.get("timeout").and_then(Value::as_f64);
    let suffix = match timeout {
        Some(timeout) if timeout != 0.0 && !timeout.is_nan() => theme.fg(
            ThemeColor::Muted,
            format!(" (timeout {}s)", js_number_to_string(timeout)),
        ),
        _ => JsString::default(),
    };
    let command = match str_value(args.get("command")) {
        None => invalid_arg_text(&theme),
        Some(command) if command.is_empty() => theme.fg(ThemeColor::ToolOutput, "..."),
        Some(command) => command,
    };
    js_format!(
        "{}{}",
        theme.fg(ThemeColor::ToolTitle, theme.bold(js_format!("$ {}", command))),
        suffix
    )
}

pub fn render_call(args: &JsValue, _theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    if context.execution_started {
        context.state.update(|state| {
            if let Value::Object(state) = state
                && !state.contains_key("startedAt")
            {
                state.insert("startedAt".into(), json!(now_millis()));
                state.shift_remove("endedAt");
            }
        });
    }
    super::render_text_component(context, format_bash_call(args))
}

const BASH_PREVIEW_LINES: usize = 5;
fn format_duration(ms: f64) -> String {
    format!("{:.1}s", ms / 1000.0)
}
fn now_millis() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as f64)
        .unwrap_or_default()
}

struct BashPreviewComponent {
    styled_output: JsString,
    theme: Theme,
}
impl Component for BashPreviewComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let preview = truncate_to_visual_lines(&self.styled_output, BASH_PREVIEW_LINES, width, 0);
        let mut lines = vec![JsString::default()];
        if preview.skipped_count > 0 {
            let hint = js_format!(
                "{} {}{}",
                self.theme.fg(
                    ThemeColor::Muted,
                    format!("... ({} earlier lines,", preview.skipped_count)
                ),
                key_hint("app.tools.expand", "to expand"),
                self.theme.fg(ThemeColor::Muted, ")")
            );
            lines.push(truncate_to_width_with(&hint, width, "...", false));
        }
        lines.extend(preview.visual_lines);
        lines
    }
    fn wants_input(&self) -> bool {
        false
    }
}

fn truncation_details(result: &AgentToolResult) -> Option<TruncationResult> {
    result
        .details
        .as_ref()?
        .get("truncation")
        .and_then(|value| pi_js_compat::js_json::from_value(value).ok())
}
fn full_output_path(result: &AgentToolResult) -> Option<JsString> {
    result.details.as_ref()?.get("fullOutputPath")?.as_string().cloned()
}
fn strip_truncation_footer(
    output: &JsString,
    truncation: Option<&TruncationResult>,
    path: Option<&JsString>,
) -> JsString {
    let Some(path) = path else {
        return output.clone();
    };
    if !truncation.is_some_and(|truncation| truncation.truncated) || !output.ends_with(']') {
        return output.clone();
    }
    let marker = JsString::from("\n\n[");
    let Some(start) = output
        .units()
        .windows(marker.len())
        .rposition(|units| units == marker.units())
    else {
        return output.clone();
    };
    if !output.slice(start, output.len()).contains(path) {
        return output.clone();
    }
    output.slice(0, start).trim_end()
}
fn format_truncation_warnings(truncation: Option<&TruncationResult>, path: Option<&JsString>) -> Option<JsString> {
    if !truncation.is_some_and(|truncation| truncation.truncated) && path.is_none() {
        return None;
    }
    let mut warnings = Vec::new();
    if let Some(path) = path {
        warnings.push(js_format!("Full output: {}", path));
    }
    if let Some(truncation) = truncation.filter(|truncation| truncation.truncated) {
        warnings.push(if truncation.truncated_by == Some(TruncatedBy::Lines) {
            format!(
                "Truncated: showing {} of {} lines",
                truncation.output_lines, truncation.total_lines
            )
            .into()
        } else {
            format!(
                "Truncated: {} lines shown ({} limit)",
                truncation.output_lines,
                format_size(truncation.max_bytes)
            )
            .into()
        });
    }
    Some(JsString::join(warnings, ". "))
}
fn append_bash_output(component: &mut Container, output: &JsString, options: &ToolRenderResultOptions, theme: &Theme) {
    if output.is_empty() {
        return;
    }
    let styled = JsString::join(
        output.split('\n').map(|line| theme.fg(ThemeColor::ToolOutput, line)),
        "\n",
    );
    let body: ComponentRef = if options.expanded {
        Rc::new(RefCell::new(Text::new(js_format!("\n{}", styled), 0, 0)))
    } else {
        Rc::new(RefCell::new(BashPreviewComponent {
            styled_output: styled,
            theme: theme.clone(),
        }))
    };
    component.add_child(body);
}
fn append_bash_duration(
    component: &mut Container,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) {
    if !options.is_partial || context.is_error {
        context.state.update(|state| {
            if let Value::Object(state) = state
                && !state.contains_key("endedAt")
            {
                state.insert("endedAt".into(), json!(now_millis()));
            }
        });
    }
    let state = context.state.get();
    if let Some(start) = state.get("startedAt").and_then(Value::as_f64) {
        let end = state.get("endedAt").and_then(Value::as_f64).unwrap_or_else(now_millis);
        let label = if options.is_partial { "Elapsed" } else { "Took" };
        component.add_child(Rc::new(RefCell::new(Text::new(
            js_format!(
                "\n{}",
                theme.fg(ThemeColor::Muted, format!("{label} {}", format_duration(end - start)))
            ),
            0,
            0,
        ))));
    }
}
pub fn render_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    let mut component = Container::new();
    let truncation = truncation_details(result);
    let path = full_output_path(result);
    let output = get_text_output(Some(result), context.show_images).trim();
    let output = if options.is_partial {
        output
    } else {
        strip_truncation_footer(&output, truncation.as_ref(), path.as_ref())
    };
    append_bash_output(&mut component, &output, options, theme);
    if let Some(warnings) = format_truncation_warnings(truncation.as_ref(), path.as_ref()) {
        component.add_child(Rc::new(RefCell::new(Text::new(
            js_format!("\n{}", theme.fg(ThemeColor::Warning, js_format!("[{}]", warnings))),
            0,
            0,
        ))));
    }
    append_bash_duration(&mut component, options, theme, context);
    Rc::new(RefCell::new(component))
}

/// `createBashToolDefinition`: the bash tool plus its renderers.
pub fn create_bash_tool_definition(cwd: impl Into<JsString>, options: BashToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(BashTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.prompt_snippet = Some(BASH_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet.into());
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

/// `createBashTool`: the bash tool wrapped with the shared result cap.
pub fn create_bash_tool(cwd: impl Into<JsString>, options: BashToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(BashTool::with_options(cwd, options)))
}
