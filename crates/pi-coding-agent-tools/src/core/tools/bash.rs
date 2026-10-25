//! Ported from `packages/coding-agent/src/core/tools/bash.ts`.
//!
//! Local shell execution with streaming output, throttled updates, timeout and
//! abort mapping, tail truncation with a full-output spill file, and the
//! runner-owned detach path backed by the detached job lifecycle controller.

use crate::utils::paths::{node_is_absolute, node_join};
use indexmap::IndexMap;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsValue;
use std::cell::RefCell;
use std::path::Path;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject as Map, JsString, JsValue as Value, encode_node_utf8};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use pi_js_compat::number::js_number_to_string;
use pi_tui::Text;
use pi_tui::tui::{Component, ComponentRef, Container};
use pi_tui::utils::truncate_to_width_with;
use serde_json::Value as SchemaValue;
use tokio::io::AsyncWriteExt;

use super::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use super::render_utils::{get_text_output, invalid_arg_text, str_value};
use super::tool_definition_wrapper::create_tool_definition_from_agent_tool;
use super::tool_definition_wrapper::wrap_agent_tool;
use super::truncate::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncatedBy, TruncationResult, format_size};
use crate::core::detached_bash_runner::{is_process_identity_alive, write_detached_bash_activation};
use crate::core::detached_job_runner::{
    DetachedJobAgentType, DetachedJobLifecycleController, DetachedJobOwnership, LaunchDetachedBashInput,
    RegisterDetachedJobInput,
};
use crate::core::extensions::types::{HandlerError, ToolDefinition, ToolRenderContext, ToolRenderResultOptions};
use crate::core::multi_agent_store::{AgentLifecycle, is_active_lifecycle};
use crate::core::tool_detach_registry::{ToolDetachHandle, ToolDetachRegistry};
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::components::visual_truncate::truncate_to_visual_lines;
use crate::modes::interactive::theme::{Theme, ThemeColor, current_theme};
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

/// Shared completion of the native child wait, including the inherited-stdio grace.
#[derive(Clone)]
pub struct BashProcessExit {
    receiver: tokio::sync::watch::Receiver<Option<Result<Option<f64>, ToolError>>>,
}

impl BashProcessExit {
    pub async fn wait(&self) -> Result<Option<f64>, ToolError> {
        let mut receiver = self.receiver.clone();
        loop {
            if let Some(result) = receiver.borrow().clone() {
                return result;
            }
            receiver
                .changed()
                .await
                .map_err(|_| ToolError::new("Bash exit observer closed without a result"))?;
        }
    }
}

/// Native child identity; Node object identity is not part of the extension API.
#[derive(Clone)]
pub struct BashChildProcess {
    pub pid: Option<u32>,
}

/// Cancellation ownership of the original command timeout, transferred to adoption.
#[derive(Clone)]
pub struct BashTimeoutHandle {
    controller: pi_js_compat::abort::AbortController,
}

impl BashTimeoutHandle {
    pub fn cancel(&self) {
        self.controller.abort(None);
    }
}

/// Source `DetachedBashProcess` values observed by an optional native adopter.
#[derive(Clone)]
pub struct DetachedBashProcess {
    pub artifacts: Option<DetachedJobArtifacts>,
    pub child: BashChildProcess,
    pub command: JsString,
    pub cwd: JsString,
    pub exit: BashProcessExit,
    pub pid: Option<u32>,
    pub started_at: f64,
    pub timeout_handle: Option<BashTimeoutHandle>,
}

pub type BashAdopt =
    Arc<dyn Fn(DetachedBashProcess) -> BoxFuture<'static, Result<BashDetachedResult, ToolError>> + Send + Sync>;

/// Detach request passed to [`BashOperations::exec`].
#[derive(Clone)]
pub struct BashDetachOptions {
    pub artifacts: Option<DetachedJobArtifacts>,
    pub job_id: Option<JsString>,
    pub signal: AbortSignal,
    pub adopt: Option<BashAdopt>,
}

/// `BashSpawnContext`: command, cwd, and environment before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashSpawnContext {
    pub command: JsString,
    pub cwd: JsString,
    pub env: IndexMap<JsString, Option<JsString>>,
}

/// `BashSpawnHook`.
pub type BashSpawnHook = Arc<dyn Fn(BashSpawnContext) -> Result<BashSpawnContext, CallbackError> + Send + Sync>;

pub type BashGetLifecycle =
    Arc<dyn Fn() -> Result<Option<Arc<DetachedJobLifecycleController>>, CallbackError> + Send + Sync>;

/// `BashBackgroundJobsOptions`: resolves the session's detached job lifecycle.
#[derive(Clone)]
pub struct BashBackgroundJobs {
    pub get_lifecycle: Option<BashGetLifecycle>,
    /// Fixed lifecycle used when `get_lifecycle` yields nothing.
    pub lifecycle: Option<Arc<DetachedJobLifecycleController>>,
}

/// Result of executing one shell command.
#[derive(Debug, Clone, PartialEq)]
pub struct BashExecResult {
    /// JavaScript number domain for custom operations; native process codes convert exactly.
    pub exit_code: Option<f64>,
    pub detached: Option<BashDetachedResult>,
}

/// Streaming output callback.
pub use crate::utils::child_process::ProcessDataCallback as BashDataCallback;
use pi_js_compat::callback_error::CallbackError;

/// Data and lifecycle hooks for one shell execution.
pub struct BashExecOptions {
    pub on_data: BashDataCallback,
    pub signal: Option<AbortSignal>,
    pub timeout: Option<f64>,
    pub env: Option<IndexMap<JsString, Option<JsString>>>,
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
    if options.signal.as_ref().is_some_and(AbortSignal::aborted) {
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

    let spawned = prepare_local_bash_command(command, cwd, &shell_config, options.env, output_file.as_ref())
        .map(|mut builder| builder.spawn());
    if let Some(file) = output_file {
        nix::unistd::close(file).map_err(|error| {
            ToolError::new(pi_js_compat::node_fs::node_fs_error(
                &std::io::Error::from(error),
                "close",
                &JsString::default(),
            ))
        })?;
    }
    let mut child = match spawned? {
        Ok(child) => child,
        Err(error) => {
            if pi_js_compat::node_child_process::spawn_error_is_async(&error)
                && let Some(artifacts) = options.detach.as_ref().and_then(|detach| detach.artifacts.as_ref())
            {
                remove_bash_artifacts(&artifacts.directory)?;
            }
            return Err(ToolError::new(
                pi_js_compat::node_fs::node_spawn_error_record(&error, &shell_config.shell).message,
            ));
        }
    };
    let child_pid = child.id();
    if let Some(pid) = child_pid {
        track_detached_child_pid(pid);
    }
    if command_from_stdin && let Some(mut stdin) = child.stdin.take() {
        let input = encode_node_utf8(command);
        tokio::spawn(async move {
            // Frozen stdin.end queues the write and ignores pipe errors; it must
            // not postpone timeout/abort observation while the pipe is full.
            let _ = stdin.write_all(input.as_bytes()).await;
            let _ = stdin.shutdown().await;
        });
    }

    let stream_active = Arc::new(AtomicBool::new(true));
    let exit = spawn_local_bash_wait(child, stream_active.clone(), options.on_data.clone());
    let mut poller = options.detach.as_ref().and_then(|detach| {
        detach
            .artifacts
            .as_ref()
            .map(|artifacts| spawn_output_poller(artifacts.output_path.clone(), Arc::clone(&options.on_data)))
    });

    let deadline = timeout_ms.map(|ms| tokio::time::Instant::now() + Duration::from_secs_f64(ms / 1000.0));
    let timeout_handle = timeout_ms.map(|_| BashTimeoutHandle {
        controller: pi_js_compat::abort::AbortController::new(),
    });
    let mut timed_out = false;
    let mut timeout_cancelled = false;
    let mut detached = None;
    let mut adoption = None;
    let mut adoption_error = None;
    let mut detach_handled = false;
    let detach_initially_aborted = options.detach.as_ref().is_some_and(|detach| detach.signal.aborted());
    let wait = exit.wait();
    tokio::pin!(wait);
    let exit_status = loop {
        let detach_signal = options.detach.as_ref().map(|detach| &detach.signal);
        tokio::select! {
            biased;
            status = &mut wait => break Some(status),
            result = wait_for_bash_adoption(&mut adoption), if adoption.is_some() => {
                match result {
                    Ok(result) => detached = Some(result),
                    Err(error) => adoption_error = Some(error),
                }
                break None;
            }
            () = cancellation(options.signal.as_ref()) => {
                if let Some(pid) = child_pid {
                    kill_process_tree(pid);
                }
                break Some((&mut wait).await);
            }
            () = cancellation(detach_signal), if !detach_handled && detach_signal.is_some() => {
                detach_handled = true;
                if let Some(poller) = poller.take() {
                    let polled = poller.poll_once(&options.on_data);
                    poller.task.abort();
                    if let Err(error) = polled {
                        if !detach_initially_aborted {
                            crate::utils::uncaught::throw_callback_uncaught(error);
                        }
                        adoption_error = Some(ToolError::new(error.message));
                        break None;
                    }
                }
                stream_active.store(false, Ordering::SeqCst);
                let process = DetachedBashProcess {
                    // Frozen's adoption object omits artifacts even when supplied.
                    artifacts: None,
                    child: BashChildProcess { pid: child_pid },
                    command: command.clone(),
                    cwd: cwd.clone(),
                    exit: exit.clone(),
                    pid: child_pid,
                    started_at: now_millis(),
                    timeout_handle: timeout_handle.clone(),
                };
                adoption = Some(match options.detach.as_ref().and_then(|detach| detach.adopt.as_ref()) {
                    Some(adopt) => adopt(process),
                    None => Box::pin(async {
                        Ok(BashDetachedResult {
                            job_id: "background".into(),
                            message: "Background job started".into(),
                            log_path: None,
                        })
                    }),
                });
            }
            expired = wait_for_bash_timeout(deadline, timeout_handle.as_ref()), if !timeout_cancelled => {
                if !expired {
                    timeout_cancelled = true;
                    continue;
                }
                timed_out = true;
                if let Some(pid) = child_pid {
                    kill_process_tree(pid);
                }
                break Some((&mut wait).await);
            }
        }
    };

    if let Some(poller) = poller.take() {
        poller.task.abort();
    }
    if detached.is_some() {
        if let (Some(deadline), Some(pid)) = (deadline, child_pid) {
            let timeout_handle = timeout_handle.clone();
            tokio::spawn(async move {
                if wait_for_bash_timeout(Some(deadline), timeout_handle.as_ref()).await {
                    kill_process_tree(pid);
                }
            });
        }
        return Ok(BashExecResult {
            exit_code: None,
            detached,
        });
    }
    if let Some(artifacts) = options.detach.as_ref().and_then(|detach| detach.artifacts.as_ref()) {
        remove_bash_artifacts(&artifacts.directory)?;
    }
    if let Some(pid) = child_pid {
        untrack_detached_child_pid(pid);
    }
    if let Some(handle) = timeout_handle {
        handle.cancel();
    }
    if let Some(error) = adoption_error {
        return Err(error);
    }
    if options.signal.as_ref().is_some_and(AbortSignal::aborted) {
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
        Some(Err(error)) => return Err(error),
        None => return Err(ToolError::new("Command did not produce an exit status")),
    };
    Ok(BashExecResult {
        exit_code: status,
        detached: None,
    })
}

fn prepare_local_bash_command(
    command: &JsString,
    cwd: &JsString,
    shell: &crate::utils::shell::ShellConfig,
    env: Option<IndexMap<JsString, Option<JsString>>>,
    output_file: Option<&std::fs::File>,
) -> Result<tokio::process::Command, ToolError> {
    let from_stdin = shell.command_transport == CommandTransport::Stdin;
    let mut args = shell.args.clone();
    if !from_stdin {
        args.push(command.clone());
    }
    let mut builder = tokio::process::Command::new(encode_node_utf8(&resolve_shell_executable(&shell.shell, cwd)));
    builder.current_dir(encode_node_utf8(cwd));
    builder.args(args.iter().map(encode_node_utf8));
    builder.stdin(if from_stdin { Stdio::piped() } else { Stdio::null() });
    let env = match env {
        Some(env) => env,
        None => load_bash_environment()?,
    };
    builder.env_clear();
    builder.envs(ordered_bash_environment(&env).into_iter().filter_map(|(name, value)| {
        value
            .as_ref()
            .map(|value| (encode_node_utf8(name), encode_node_utf8(value)))
    }));
    builder.stdout(clone_bash_output_stdio(output_file)?);
    builder.stderr(clone_bash_output_stdio(output_file)?);
    #[cfg(unix)]
    builder.process_group(0);
    pi_js_compat::node_child_process::validate_spawn_arguments(&shell.shell, &args)
        .map_err(|error| ToolError::new(error.message.clone()))?;
    validate_bash_environment(&env)?;
    Ok(builder)
}

fn load_bash_environment() -> Result<IndexMap<JsString, Option<JsString>>, ToolError> {
    Ok(get_shell_env()
        .map_err(|error| ToolError::new(error.message().clone()))?
        .into_iter()
        .map(|(name, value)| (name, Some(value)))
        .collect())
}

fn validate_bash_environment(env: &IndexMap<JsString, Option<JsString>>) -> Result<(), ToolError> {
    for (name, value) in ordered_bash_environment(env) {
        let Some(value) = value else { continue };
        let label = js_format!("options.env['{}']", name);
        for argument in [name, value] {
            pi_js_compat::node_child_process::validate_string_argument(
                label.clone(),
                "must be a string without null bytes",
                argument,
            )
            .map_err(|error| ToolError::new(error.message.clone()))?;
        }
    }
    Ok(())
}

fn ordered_bash_environment(env: &IndexMap<JsString, Option<JsString>>) -> Vec<(&JsString, &Option<JsString>)> {
    let keys: Map = env.keys().map(|name| (name.clone(), Value::Null)).collect();
    pi_js_compat::js_json::object_entries_lossless(&keys)
        .into_iter()
        .map(|(name, _)| env.get_key_value(name).expect("enumerated environment key exists"))
        .collect()
}

fn clone_bash_output_stdio(file: Option<&std::fs::File>) -> Result<Stdio, ToolError> {
    match file {
        Some(file) => file
            .try_clone()
            .map(Stdio::from)
            .map_err(|error| ToolError::new(error.to_string())),
        None => Ok(Stdio::piped()),
    }
}

fn remove_bash_artifacts(path: &JsString) -> Result<(), ToolError> {
    if let Some(error) = pi_js_compat::node_fs::node_path_argument_error(path) {
        return Err(ToolError::new(error.message));
    }
    let encoded = encode_node_utf8(path);
    let metadata = match std::fs::symlink_metadata(&encoded) {
        Ok(metadata) => metadata,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(());
        }
        Err(error) => {
            return Err(ToolError::new(pi_js_compat::node_fs::node_fs_error(
                &error, "lstat", path,
            )));
        }
    };
    let (result, syscall) = if metadata.is_dir() {
        (std::fs::remove_dir_all(encoded), "rmdir")
    } else {
        (std::fs::remove_file(encoded), "unlink")
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ToolError::new(pi_js_compat::node_fs::node_fs_error(
            &error, syscall, path,
        ))),
    }
}

fn resolve_shell_executable(shell: &JsString, cwd: &JsString) -> JsString {
    if node_is_absolute(shell) || !shell.contains('/') {
        shell.clone()
    } else {
        node_join(cwd, shell)
    }
}

fn open_output_file(path: &JsString) -> Result<std::fs::File, ToolError> {
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(error) = pi_js_compat::node_fs::node_path_argument_error(path) {
        return Err(ToolError::new(error.message));
    }
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(encode_node_utf8(path))
        .map_err(|error| ToolError::new(pi_js_compat::node_fs::node_fs_error(&error, "open", path)))
}

fn spawn_output_poller(path: JsString, on_data: BashDataCallback) -> DurableOutputPoller {
    let offset = Arc::new(AtomicU64::new(0));
    let poll_offset = Arc::clone(&offset);
    let poll_path = path.clone();
    let task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            if let Err(error) = poll_durable_output(&poll_path, &poll_offset, &on_data) {
                crate::utils::uncaught::throw_callback_uncaught(error);
            }
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
    fn poll_once(&self, on_data: &BashDataCallback) -> Result<(), CallbackError> {
        poll_durable_output(&self.path, &self.offset, on_data)
    }
}

fn poll_durable_output(path: &JsString, offset: &AtomicU64, on_data: &BashDataCallback) -> Result<(), CallbackError> {
    let bytes = std::fs::read(encode_node_utf8(path)).map_err(|error| {
        CallbackError::from(pi_js_compat::node_error::NativeNodeError::from(
            pi_js_compat::node_fs::node_read_error_record(
                &error,
                path,
                pi_js_compat::node_fs::NodeReadApi::ReadFileSync,
            ),
        ))
    })?;
    loop {
        let current = offset.load(Ordering::SeqCst);
        let length = bytes.len() as u64;
        if length <= current {
            return Ok(());
        }
        if offset
            .compare_exchange(current, length, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return on_data(&bytes[current as usize..]);
        }
    }
}

async fn cancellation(signal: Option<&AbortSignal>) {
    match signal {
        Some(signal) => {
            signal.wait().await;
        }
        None => std::future::pending().await,
    }
}

async fn wait_for_bash_timeout(deadline: Option<tokio::time::Instant>, handle: Option<&BashTimeoutHandle>) -> bool {
    let Some(deadline) = deadline else {
        return std::future::pending().await;
    };
    let signal = handle.expect("validated timeout has a handle").controller.signal();
    tokio::select! {
        () = tokio::time::sleep_until(deadline) => true,
        _ = signal.wait() => false,
    }
}

async fn wait_for_bash_adoption(
    adoption: &mut Option<BoxFuture<'static, Result<BashDetachedResult, ToolError>>>,
) -> Result<BashDetachedResult, ToolError> {
    adoption.as_mut().expect("adoption select is enabled").await
}

fn spawn_local_bash_wait(
    mut child: tokio::process::Child,
    stream_active: Arc<AtomicBool>,
    on_data: BashDataCallback,
) -> BashProcessExit {
    let (sender, receiver) = tokio::sync::watch::channel(None);
    tokio::spawn(async move {
        let result = crate::utils::child_process::wait_for_child_process_streaming(
            &mut child,
            Arc::new(move |bytes| {
                if stream_active.load(Ordering::SeqCst) {
                    on_data(bytes)
                } else {
                    Ok(())
                }
            }),
        )
        .await
        .map(|output| output.status.map(f64::from))
        .map_err(|error| ToolError::new(error.to_string()));
        sender.send_replace(Some(result));
    });
    BashProcessExit { receiver }
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
    /// Hook to adjust command, cwd, or env before execution.
    pub spawn_hook: Option<BashSpawnHook>,
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
    spawn_hook: Option<BashSpawnHook>,
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
            spawn_hook: options.spawn_hook,
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

    fn description(&self) -> pi_js_compat::js_json::JsString {
        DESCRIPTION.as_str().into()
    }

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "bash".into()
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: JsValue,
        signal: Option<AbortSignal>,
        on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let cwd = self.cwd.clone();
        let operations = Arc::clone(&self.operations);
        let custom_operations = self.custom_operations;
        let command_prefix = self.command_prefix.clone();
        let shell_path = self.shell_path.clone();
        let spawn_hook = self.spawn_hook.clone();
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
                    spawn_hook,
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
    spawn_hook: Option<BashSpawnHook>,
    configured_detach: Option<BashDetachOptions>,
    detach_registry: Option<Arc<ToolDetachRegistry>>,
    background_jobs: Option<Arc<BashBackgroundJobs>>,
}

fn resolve_spawn_context(
    command: JsString,
    cwd: &JsString,
    spawn_hook: Option<&BashSpawnHook>,
) -> Result<BashSpawnContext, ToolError> {
    let env = load_bash_environment()?;
    let base = BashSpawnContext {
        command,
        cwd: cwd.to_owned(),
        env,
    };
    match spawn_hook {
        Some(spawn_hook) => spawn_hook(base).map_err(|error| ToolError::new(error.message)),
        None => Ok(base),
    }
}

async fn execute_bash(
    cwd: &JsString,
    options: BashExecutionOptions,
    tool_call_id: String,
    params: JsValue,
    signal: Option<AbortSignal>,
    on_update: AgentToolUpdateCallback,
) -> Result<AgentToolResult, ToolError> {
    let command = params
        .get("command")
        .and_then(Value::as_string)
        .ok_or_else(|| ToolError::new("bash tool requires a string command"))?
        .to_owned();
    let timeout = params.get("timeout").and_then(Value::as_f64);
    let resolved_command = match &options.command_prefix {
        Some(prefix) if !prefix.is_empty() => JsString::from(prefix) + "\n" + command,
        _ => command,
    };
    let spawn_context = resolve_spawn_context(resolved_command, cwd, options.spawn_hook.as_ref())?;
    let lifecycle = if let Some(jobs) = options.background_jobs.as_ref() {
        let current = match &jobs.get_lifecycle {
            Some(get) => get().map_err(|error| ToolError::new(error.message))?,
            None => None,
        };
        current.or_else(|| jobs.lifecycle.clone())
    } else {
        None
    };
    let detach_controller =
        if options.configured_detach.is_none() && lifecycle.is_some() && options.detach_registry.is_some() {
            Some(pi_js_compat::abort::AbortController::new())
        } else {
            None
        };
    let detach = options.configured_detach.clone().or_else(|| {
        detach_controller.as_ref().map(|signal| BashDetachOptions {
            artifacts: None,
            job_id: None,
            signal: signal.signal(),
            adopt: None,
        })
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
    throttle.emit_initial()?;
    let accepting_output = Arc::new(AtomicBool::new(true));

    let on_data: BashDataCallback = {
        let output = Arc::clone(&output);
        let throttle = Arc::clone(&throttle);
        let accepting_output = Arc::clone(&accepting_output);
        Arc::new(move |data: &[u8]| {
            if !accepting_output.load(Ordering::SeqCst) {
                return Ok(());
            }
            output
                .lock()
                .expect("output lock")
                .append(data)
                .map_err(CallbackError::from)?;
            throttle
                .schedule(&output)
                .map_err(|error| CallbackError::new(error.message))
        })
    };

    let use_detached_runner = !custom_operations && lifecycle.is_some() && detach.is_some();
    let runner_activated = Arc::new(AtomicBool::new(false));
    let detach_subscription = detach_controller.as_ref().and_then(|signal| {
        options.detach_registry.as_ref().map(|registry| {
            registry.register(Arc::new(BashDetachHandle {
                signal: signal.clone(),
                use_detached_runner,
                runner_activated: runner_activated.clone(),
            }))
        })
    });
    let result = if use_detached_runner {
        execute_runner_owned_bash(
            &command,
            &spawn_context.cwd,
            RunnerOwnedOptions {
                detach: detach.clone().expect("detach options"),
                env: env.clone(),
                lifecycle: lifecycle.clone().expect("lifecycle"),
                on_activated: Arc::new(move || runner_activated.store(true, Ordering::SeqCst)),
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
                &spawn_context.cwd,
                BashExecOptions {
                    on_data: Arc::clone(&on_data),
                    signal: signal.clone(),
                    timeout,
                    env: Some(env.clone()),
                    detach,
                },
            )
            .await
            .map(|result| BashRunnerResult {
                exit_code: result.exit_code.map(Value::Number).unwrap_or(Value::Null),
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
            if result.exit_code != Value::Number(0.0) && result.exit_code != Value::Null {
                let code = pi_js_compat::js_json::to_js_string(&result.exit_code).map_err(ToolError::new)?;
                return Err(ToolError::new(append_status(
                    &text,
                    js_format!("Command exited with code {}", code),
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
            if error.message.starts_with("timeout:") {
                let timeout_secs = error.message.split(':').nth(1).unwrap_or_default();
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
    signal: pi_js_compat::abort::AbortController,
    use_detached_runner: bool,
    runner_activated: Arc<AtomicBool>,
}

impl ToolDetachHandle for BashDetachHandle {
    fn detach(&self) -> Result<bool, pi_js_compat::callback_error::CallbackError> {
        // Frozen bash.ts:737-746: this callback only aborts and returns activation state.
        if self.signal.signal().aborted() {
            return Ok(false);
        }
        self.signal.abort(None);
        Ok(!self.use_detached_runner || self.runner_activated.load(Ordering::SeqCst))
    }
}

#[derive(Clone)]
struct RunnerOwnedOptions {
    detach: BashDetachOptions,
    env: IndexMap<JsString, Option<JsString>>,
    lifecycle: Arc<DetachedJobLifecycleController>,
    on_activated: Arc<dyn Fn() + Send + Sync>,
    on_data: BashDataCallback,
    shell_path: Option<JsString>,
    signal: Option<AbortSignal>,
    timeout: Option<f64>,
    tool_call_id: String,
}

#[derive(Clone)]
struct BashRunnerResult {
    exit_code: JsValue,
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
            env: ordered_bash_environment(&options.env)
                .into_iter()
                .filter_map(|(name, value)| value.as_ref().map(|value| (name.clone(), value.clone())))
                .collect(),
            timeout_ms: resolve_timeout_ms(options.timeout)?,
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
    if let Some(output_path) = output_path.filter(|path| !path.is_empty())
        && Path::new(&encode_node_utf8(&output_path)).exists()
    {
        let output = std::fs::read(encode_node_utf8(&output_path))
            .map_err(|error| ToolError::new(pi_js_compat::node_fs::node_read_error(&error, &output_path)))?;
        (options.on_data)(&output).map_err(|error| ToolError::new(error.message))?;
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
            timeout
                .map(format_timeout_seconds)
                .unwrap_or_else(|| "undefined".to_owned())
        )));
    }
    Ok(BashRunnerResult {
        exit_code: Value::Number(if agent.lifecycle == AgentLifecycle::Completed {
            0.0
        } else {
            1.0
        }),
        detached: None,
    })
}

fn format_timeout_seconds(timeout: f64) -> String {
    js_number_to_string(timeout)
}

#[derive(Default)]
struct ForegroundBashObservation {
    output_offset: AtomicU64,
    aborted: AtomicBool,
    activation: Mutex<Option<Result<BashRunnerResult, ToolError>>>,
}

impl ForegroundBashObservation {
    fn subscribe_cancellation(
        self: &Arc<Self>,
        signal: Option<&AbortSignal>,
        runner_pid: u32,
    ) -> Option<pi_js_compat::abort::AbortSubscription> {
        let signal = signal?;
        let cancel = self.clone();
        match signal.subscribe(move |_| cancel.request_cancellation(runner_pid)) {
            Ok(subscription) => Some(subscription),
            Err(_already_aborted_reason) => {
                self.request_cancellation(runner_pid);
                None
            }
        }
    }

    fn subscribe_detach(
        self: &Arc<Self>,
        runner: &crate::core::detached_job_runner::LaunchedDetachedBashJob,
        cwd: &JsString,
        options: &RunnerOwnedOptions,
    ) -> Option<pi_js_compat::abort::AbortSubscription> {
        let activate = self.clone();
        let runner = runner.clone();
        let cwd = cwd.clone();
        let settings = options.clone();
        // An already-aborted reason is not an execution failure: addEventListener
        // does not replay, and the observer loop handles the pre-aborted detach.
        options
            .detach
            .signal
            .subscribe(move |_| activate.activate_on_detach(&runner, &cwd, &settings))
            .ok()
    }

    fn activation_result(&self) -> Option<Result<BashRunnerResult, ToolError>> {
        self.activation.lock().expect("bash activation lock").clone()
    }

    fn request_cancellation(&self, runner_pid: u32) {
        if self.activation_result().is_some_and(|result| result.is_ok()) {
            return;
        }
        self.aborted.store(true, Ordering::SeqCst);
        terminate_foreground_bash_runner(runner_pid);
    }

    fn forward_output(
        &self,
        runner: &crate::core::detached_job_runner::LaunchedDetachedBashJob,
        on_data: &BashDataCallback,
    ) -> Result<(), CallbackError> {
        let offset = self.output_offset.load(Ordering::SeqCst);
        let offset = forward_bash_runner_output(runner, offset, on_data)?;
        // Source assigns after onData: a synchronous detach may re-enter with the old offset.
        self.output_offset.store(offset, Ordering::SeqCst);
        Ok(())
    }

    fn activate_on_detach(
        &self,
        runner: &crate::core::detached_job_runner::LaunchedDetachedBashJob,
        cwd: &JsString,
        options: &RunnerOwnedOptions,
    ) {
        if self.activation_result().is_some_and(|result| result.is_ok()) {
            return;
        }
        let completion = read_foreground_bash_completion(&runner.foreground_completion_path)
            .unwrap_or_else(|error| crate::utils::uncaught::throw_native_uncaught(*error));
        if completion.is_some_and(|value| pi_js_compat::js_json::is_truthy(&value)) {
            return;
        }
        self.forward_output(runner, &options.on_data)
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        match activate_detached_bash_runner(runner, cwd, options) {
            Ok(Some((_ownership, result))) => {
                *self.activation.lock().expect("bash activation lock") = Some(Ok(result));
                (options.on_activated)();
            }
            Ok(None) => {}
            Err(error) => *self.activation.lock().expect("bash activation lock") = Some(Err(error)),
        }
    }
}

async fn observe_foreground_bash_runner(
    runner: crate::core::detached_job_runner::LaunchedDetachedBashJob,
    cwd: &JsString,
    options: &RunnerOwnedOptions,
) -> Result<BashRunnerResult, ToolError> {
    let observation = Arc::new(ForegroundBashObservation::default());
    let _abort_subscription = observation.subscribe_cancellation(options.signal.as_ref(), runner.runner_pid);
    let _detach_subscription = observation.subscribe_detach(&runner, cwd, options);
    loop {
        if let Some(activation) = observation.activation_result() {
            return activation;
        }
        observation
            .forward_output(&runner, &options.on_data)
            .map_err(|error| ToolError::new(error.message))?;
        if let Some(completion) =
            settle_foreground_bash_runner(&runner, observation.aborted.load(Ordering::SeqCst), options.timeout)?
        {
            return Ok(completion);
        }
        if options.detach.signal.aborted() {
            observation.activate_on_detach(&runner, cwd, options);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn read_bash_runner_file(path: &JsString) -> Result<Vec<u8>, Box<NativeNodeError>> {
    std::fs::read(encode_node_utf8(path)).map_err(|error| {
        Box::new(NativeNodeError::from(pi_js_compat::node_fs::node_read_error_record(
            &error,
            path,
            pi_js_compat::node_fs::NodeReadApi::ReadFileSync,
        )))
    })
}

fn read_bash_runner_text(path: &JsString) -> Result<JsString, Box<NativeNodeError>> {
    let bytes = read_bash_runner_file(path)?;
    // readFileSync(path, "utf8") repairs OS bytes like Buffer.toString, retaining BOM.
    Ok(JsString::from(String::from_utf8_lossy(&bytes).as_ref()))
}

fn forward_bash_runner_output(
    runner: &crate::core::detached_job_runner::LaunchedDetachedBashJob,
    output_offset: u64,
    on_data: &BashDataCallback,
) -> Result<u64, CallbackError> {
    let path = &runner.artifacts.output_path;
    if !Path::new(&encode_node_utf8(path)).exists() {
        return Ok(output_offset);
    }
    let output = read_bash_runner_file(path).map_err(|error| CallbackError::from(*error))?;
    if output.len() as u64 <= output_offset {
        return Ok(output_offset);
    }
    on_data(&output[output_offset as usize..])?;
    Ok(output.len() as u64)
}

fn result_from_foreground_bash_completion(
    completion: &JsValue,
    aborted: bool,
    timeout: Option<f64>,
) -> Result<BashRunnerResult, ToolError> {
    if completion.get("timedOut").is_some_and(pi_js_compat::js_json::is_truthy) {
        return Err(ToolError::new(format!(
            "timeout:{}",
            timeout
                .map(format_timeout_seconds)
                .unwrap_or_else(|| "undefined".to_owned())
        )));
    }
    if aborted
        || completion
            .get("cancelReason")
            .is_some_and(pi_js_compat::js_json::is_truthy)
    {
        return Err(ToolError::new("aborted"));
    }
    Ok(BashRunnerResult {
        exit_code: completion
            .get("exitCode")
            .filter(|value| !value.is_null())
            .cloned()
            .unwrap_or(Value::Number(1.0)),
        detached: None,
    })
}

fn settle_foreground_bash_runner(
    runner: &crate::core::detached_job_runner::LaunchedDetachedBashJob,
    aborted: bool,
    timeout: Option<f64>,
) -> Result<Option<BashRunnerResult>, ToolError> {
    let completion = read_foreground_bash_completion(&runner.foreground_completion_path)
        .map_err(|error| ToolError::new(error.message))?;
    if let Some(completion) = completion.filter(pi_js_compat::js_json::is_truthy) {
        return result_from_foreground_bash_completion(&completion, aborted, timeout).map(Some);
    }
    if is_process_identity_alive(&runner.process_identity) {
        return Ok(None);
    }
    let runner_error_path = js_format!("{}.runner-error", runner.manifest_path);
    if Path::new(&encode_node_utf8(&runner_error_path)).exists() {
        let message = read_bash_runner_text(&runner_error_path).map_err(|error| ToolError::new(error.message))?;
        return Err(ToolError::new(message.trim()));
    }
    if aborted {
        return Err(ToolError::new("aborted"));
    }
    Err(ToolError::new("Detached Bash runner exited without producing a result"))
}

fn read_foreground_bash_completion(path: &JsString) -> Result<Option<JsValue>, Box<NativeNodeError>> {
    if !Path::new(&encode_node_utf8(path)).exists() {
        return Ok(None);
    }
    let text = read_bash_runner_text(path)?;
    pi_js_compat::js_json::parse_lossless(text).map(Some).map_err(|error| {
        Box::new(NativeNodeError::from_constructor(
            NativeErrorClass::SyntaxError,
            error.into_message(),
        ))
    })
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
            exit_code: Value::Null,
            detached: Some(BashDetachedResult {
                job_id: runner.job_id.clone(),
                message: js_format!("Detached bash command as background job {}.", runner.job_id),
                log_path: Some(runner.artifacts.output_path.clone()),
            }),
        },
    )))
}

fn terminate_foreground_bash_runner(pid: u32) {
    kill_process_tree(pid);
}

fn finish_output(
    output: &Arc<Mutex<OutputAccumulator>>,
    throttle: &Arc<UpdateThrottle>,
) -> Result<super::output_accumulator::OutputSnapshot, ToolError> {
    let mut accumulator = output.lock().expect("output lock");
    accumulator.finish().map_err(|error| ToolError::new(error.message))?;
    throttle.mark_finished();
    let snapshot = accumulator
        .snapshot(SnapshotOptions {
            persist_if_truncated: true,
        })
        .map_err(|error| ToolError::new(error.message))?;
    if throttle.take_dirty() {
        throttle.emit_snapshot(&snapshot)?;
    }
    accumulator
        .close_temp_file()
        .map_err(|error| ToolError::new(error.message.clone()))?;
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
    details.insert("backgroundJobId".into(), Value::String(detached.job_id.clone()));
    if let Some(log_path) = detached.log_path.as_ref().filter(|path| !path.is_empty()) {
        details.insert("fullOutputPath".into(), Value::String(log_path.clone()));
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

    fn emit_initial(&self) -> Result<(), ToolError> {
        (self.on_update)(AgentToolResult {
            content: Vec::new(),
            details: None,
            is_error: None,
            terminate: None,
        })
    }

    fn mark_finished(&self) {
        self.finished.store(true, Ordering::SeqCst);
    }

    fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::SeqCst)
    }

    fn schedule(self: &Arc<Self>, output: &Arc<Mutex<OutputAccumulator>>) -> Result<(), ToolError> {
        if self.finished.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.dirty.store(true, Ordering::SeqCst);
        let elapsed = self.last_update_at.lock().expect("throttle lock").elapsed();
        if elapsed.as_millis() >= u128::from(BASH_UPDATE_THROTTLE_MS) {
            return self.emit_now(output);
        }
        if self.timer_running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let throttle = Arc::clone(self);
        let output = Arc::clone(output);
        let delay = Duration::from_millis(BASH_UPDATE_THROTTLE_MS).saturating_sub(elapsed);
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            throttle.timer_running.store(false, Ordering::SeqCst);
            throttle.emit_now(&output).unwrap_or_else(|error| {
                crate::utils::uncaught::throw_callback_uncaught(CallbackError::new(error.message))
            });
        });
        Ok(())
    }

    fn emit_now(&self, output: &Arc<Mutex<OutputAccumulator>>) -> Result<(), ToolError> {
        if self.finished.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.dirty.store(false, Ordering::SeqCst);
        *self.last_update_at.lock().expect("throttle lock") = Instant::now();
        let snapshot = output
            .lock()
            .expect("output lock")
            .snapshot(SnapshotOptions {
                persist_if_truncated: true,
            })
            .map_err(|error| ToolError::new(error.message))?;
        self.emit_snapshot(&snapshot)
    }

    fn emit_snapshot(&self, snapshot: &super::output_accumulator::OutputSnapshot) -> Result<(), ToolError> {
        let mut details = Map::new();
        if snapshot.truncation.truncated {
            details.insert(
                "truncation".into(),
                pi_js_compat::js_json::to_value(&snapshot.truncation).expect("truncation metadata must serialize"),
            );
        }
        if let Some(path) = &snapshot.full_output_path {
            details.insert("fullOutputPath".into(), Value::String(path.clone()));
        }
        (self.on_update)(AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(snapshot.content.clone()))],
            details: Some(Value::Object(details)),
            is_error: None,
            terminate: None,
        })
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

pub fn render_call(args: &JsValue, _theme: &Theme, context: &ToolRenderContext) -> Result<ComponentRef, HandlerError> {
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
    super::render_utils::set_text_component(format_bash_call(args), context)
}

const BASH_PREVIEW_LINES: usize = 5;
fn format_duration(ms: f64) -> String {
    format!("{}s", pi_js_compat::number::to_fixed(ms / 1000.0, 1))
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
    result
        .details
        .as_ref()?
        .get("fullOutputPath")?
        .as_string()
        .filter(|path| !path.is_empty())
        .cloned()
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
fn update_bash_render_timer(options: &ToolRenderResultOptions, context: &ToolRenderContext) {
    if options.is_partial && context.state.get().get("startedAt").is_some() {
        context
            .state
            .start_interval(Duration::from_secs(1), context.invalidate.clone());
    }
    if !options.is_partial || context.is_error {
        context.state.stop_interval();
        context.state.update(|state| {
            if let Value::Object(state) = state
                && !state.contains_key("endedAt")
            {
                state.insert("endedAt".into(), json!(now_millis()));
            }
        });
    }
}

fn append_bash_duration(
    component: &mut Container,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) {
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
) -> Result<ComponentRef, HandlerError> {
    update_bash_render_timer(options, context);
    let component = context
        .last_component
        .clone()
        .unwrap_or_else(|| Rc::new(RefCell::new(Container::new())));
    {
        let mut borrowed = component.borrow_mut();
        let container = borrowed
            .as_any_mut()
            .and_then(|component| component.downcast_mut::<Container>())
            .ok_or_else(|| {
                HandlerError::from(pi_js_compat::node_error::NativeNodeError::from_constructor(
                    pi_js_compat::node_error::NativeErrorClass::TypeError,
                    "component.clear is not a function",
                ))
            })?;
        container.clear();
        rebuild_bash_result(container, result, options, theme, context);
    }
    Ok(component)
}

fn rebuild_bash_result(
    component: &mut Container,
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) {
    let truncation = truncation_details(result);
    let path = full_output_path(result);
    let output = get_text_output(Some(result), context.show_images).trim();
    let output = if options.is_partial {
        output
    } else {
        strip_truncation_footer(&output, truncation.as_ref(), path.as_ref())
    };
    append_bash_output(component, &output, options, theme);
    if let Some(warnings) = format_truncation_warnings(truncation.as_ref(), path.as_ref()) {
        component.add_child(Rc::new(RefCell::new(Text::new(
            js_format!("\n{}", theme.fg(ThemeColor::Warning, js_format!("[{}]", warnings))),
            0,
            0,
        ))));
    }
    append_bash_duration(component, options, theme, context);
}

/// `createBashToolDefinition`: the bash tool plus its renderers.
pub fn create_bash_tool_definition(cwd: impl Into<JsString>, options: BashToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(BashTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.prompt_snippet = Some(BASH_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet.into());
    definition.render_call = Some(Arc::new(render_call));
    definition.render_result = Some(Arc::new(render_result));
    definition
}

/// `createBashTool`: the bash tool wrapped with the shared result cap.
pub fn create_bash_tool(cwd: impl Into<JsString>, options: BashToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(BashTool::with_options(cwd, options)))
}

#[cfg(all(test, unix))]
#[path = "bash_process_tests.rs"]
mod process_tests;
