//! Ported from `packages/coding-agent/src/core/tools/bash.ts`.
//!
//! Local shell execution with streaming output, throttled updates, timeout and
//! abort mapping, and tail truncation with a full-output spill file. Renderers
//! are pending `theme.ts`. The runner-owned detach path needs the unported
//! `detached-job-runner` lifecycle controller.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_tui::Text;
use pi_tui::tui::ComponentRef;
use serde_json::{Map, Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use super::node_fs::node_error_code;
use super::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use super::render_utils::{invalid_arg_text, str_value};
use super::tool_definition_wrapper::wrap_agent_tool;
use super::truncate::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncatedBy, format_size};
use crate::core::extensions::types::{ExtensionTheme, ToolDefinition, ToolRenderContext};
use crate::core::extensions::wrapper::create_tool_definition_from_agent_tool;
use crate::utils::shell::{
    CommandTransport, get_shell_config, get_shell_env, kill_process_tree, track_detached_child_pid,
    untrack_detached_child_pid,
};

const MAX_TIMEOUT_MS: f64 = 2_147_483_647.0;
const MAX_TIMEOUT_SECONDS: f64 = MAX_TIMEOUT_MS / 1000.0;
const BASH_UPDATE_THROTTLE_MS: u64 = 100;
const DESCRIPTION_PREFIX: &str =
    "Execute a bash command in the current working directory. Returns stdout and stderr. Output is truncated to last";

/// Output artifact paths for a detached job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedJobArtifacts {
    pub directory: String,
    pub output_path: String,
}

/// A command that moved to the background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashDetachedResult {
    pub job_id: String,
    pub message: String,
    pub log_path: Option<String>,
}

/// Detach request passed to [`BashOperations::exec`].
#[derive(Clone)]
pub struct BashDetachOptions {
    pub artifacts: Option<DetachedJobArtifacts>,
    pub job_id: Option<String>,
    pub signal: CancellationToken,
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
    pub env: BTreeMap<String, String>,
    pub detach: Option<BashDetachOptions>,
}

/// Pluggable shell execution. Override to delegate commands to a remote system.
pub trait BashOperations: Send + Sync {
    fn exec<'a>(
        &'a self,
        command: &'a str,
        cwd: &'a str,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>>;
}

/// Local shell execution.
#[derive(Debug, Default, Clone)]
pub struct LocalBashOperations {
    shell_path: Option<String>,
}

impl LocalBashOperations {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_shell_path(shell_path: impl Into<String>) -> Self {
        Self {
            shell_path: Some(shell_path.into()),
        }
    }
}

/// Resolves the shell, validates the working directory, and runs the command.
pub fn create_local_bash_operations(shell_path: Option<&str>) -> LocalBashOperations {
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
            "Invalid timeout: maximum is {MAX_TIMEOUT_SECONDS} seconds"
        )));
    }
    Ok(Some(timeout_ms))
}

impl BashOperations for LocalBashOperations {
    fn exec<'a>(
        &'a self,
        command: &'a str,
        cwd: &'a str,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move { exec_local(self.shell_path.as_deref(), command, cwd, options).await })
    }
}

async fn exec_local(
    shell_path: Option<&str>,
    command: &str,
    cwd: &str,
    options: BashExecOptions,
) -> Result<BashExecResult, ToolError> {
    let timeout_ms = resolve_timeout_ms(options.timeout)?;
    if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ToolError::new("aborted"));
    }
    let shell_config = get_shell_config(shell_path).map_err(|error| ToolError::new(error.to_string()))?;
    if tokio::fs::metadata(cwd).await.is_err() {
        return Err(ToolError::new(format!(
            "Working directory does not exist: {cwd}\nCannot execute bash commands."
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

    let mut command_builder = tokio::process::Command::new(resolve_shell_executable(&shell_config.shell, cwd));
    command_builder.current_dir(cwd);
    if command_from_stdin {
        command_builder.args(&shell_config.args);
        command_builder.stdin(Stdio::piped());
    } else {
        command_builder.args(&shell_config.args);
        command_builder.arg(command);
        command_builder.stdin(Stdio::null());
    }
    command_builder.env_clear();
    command_builder.envs(&options.env);
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
        .map_err(|error| ToolError::new(format!("spawn {} {}", shell_config.shell, node_error_code(&error))))?;
    let child_pid = child.id();
    if let Some(pid) = child_pid {
        track_detached_child_pid(pid);
    }
    if command_from_stdin && let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(command.as_bytes()).await;
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
    let mut poll_task = output_file.as_ref().and_then(|_| {
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
                detached = Some(BashDetachedResult {
                    job_id: options
                        .detach
                        .as_ref()
                        .and_then(|detach| detach.job_id.clone())
                        .unwrap_or_else(|| "background".to_owned()),
                    message: "Background job started".to_owned(),
                    log_path: options
                        .detach
                        .as_ref()
                        .and_then(|detach| detach.artifacts.as_ref())
                        .map(|artifacts| artifacts.output_path.clone()),
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

    if let Some(task) = poll_task.take() {
        task.abort();
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
    if timed_out {
        return Err(ToolError::new(format!(
            "timeout:{}",
            options.timeout.unwrap_or_default()
        )));
    }
    if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ToolError::new("aborted"));
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

fn resolve_shell_executable(shell: &str, cwd: &str) -> String {
    if Path::new(shell).is_absolute() || !shell.contains(['/', '\\']) {
        return shell.to_owned();
    }
    Path::new(cwd).join(shell).to_string_lossy().into_owned()
}

fn open_output_file(path: &str) -> Result<std::fs::File, ToolError> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| ToolError::new(format!("open {path} {}", node_error_code(&error))))
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

fn spawn_output_poller(path: String, on_data: BashDataCallback) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut offset = 0_u64;
        loop {
            tokio::time::sleep(Duration::from_millis(25)).await;
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            if bytes.len() as u64 > offset {
                on_data(&bytes[offset as usize..]);
                offset = bytes.len() as u64;
            }
        }
    })
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
    pub command_prefix: Option<String>,
    /// Shell executable override for local execution.
    pub shell_path: Option<String>,
    /// Whether to expose `PI_*` session environment variables. Default: true.
    pub expose_session_environment: Option<bool>,
}

/// Executes shell commands.
pub struct BashTool {
    cwd: String,
    parameters: Value,
    operations: Arc<dyn BashOperations>,
    command_prefix: Option<String>,
    expose_session_environment: bool,
}

impl BashTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_options(cwd, BashToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<String>, options: BashToolOptions) -> Self {
        let operations = options
            .operations
            .unwrap_or_else(|| Arc::new(create_local_bash_operations(options.shell_path.as_deref())));
        Self {
            cwd: cwd.into(),
            parameters: bash_schema(),
            operations,
            command_prefix: options.command_prefix,
            expose_session_environment: options.expose_session_environment.unwrap_or(true),
        }
    }
}

fn bash_schema() -> Value {
    json!({
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

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "bash"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let cwd = self.cwd.clone();
        let operations = Arc::clone(&self.operations);
        let command_prefix = self.command_prefix.clone();
        let expose_session_environment = self.expose_session_environment;
        Box::pin(async move {
            execute_bash(
                &cwd,
                operations,
                command_prefix,
                expose_session_environment,
                params,
                signal,
                on_update,
            )
            .await
        })
    }
}

fn shell_env(expose_session_environment: bool) -> BTreeMap<String, String> {
    let mut env = get_shell_env();
    for name in [
        "PI_SESSION_ID",
        "PI_SESSION_FILE",
        "PI_PROVIDER",
        "PI_MODEL",
        "PI_REASONING_LEVEL",
    ] {
        env.remove(name);
    }
    let _ = expose_session_environment;
    env
}

async fn execute_bash(
    cwd: &str,
    operations: Arc<dyn BashOperations>,
    command_prefix: Option<String>,
    expose_session_environment: bool,
    params: Value,
    signal: Option<CancellationToken>,
    on_update: AgentToolUpdateCallback,
) -> Result<AgentToolResult, ToolError> {
    let command = params
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("bash tool requires a string command"))?
        .to_owned();
    let timeout = params.get("timeout").and_then(Value::as_f64);
    let resolved_command = match &command_prefix {
        Some(prefix) => format!("{prefix}\n{command}"),
        None => command,
    };
    let env = shell_env(expose_session_environment);
    let output = Arc::new(Mutex::new(OutputAccumulator::new(OutputAccumulatorOptions {
        temp_file_prefix: "pi-bash".to_owned(),
        ..OutputAccumulatorOptions::default()
    })));
    let throttle = Arc::new(UpdateThrottle::new(on_update));
    throttle.emit_initial();
    let accepting_output = Arc::new(AtomicBool::new(true));

    let on_data = {
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

    let result = operations
        .exec(
            &resolved_command,
            cwd,
            BashExecOptions {
                on_data,
                signal: signal.clone(),
                timeout,
                env,
                detach: None,
            },
        )
        .await;

    accepting_output.store(false, Ordering::SeqCst);
    let snapshot = finish_output(&output, &throttle);
    match result {
        Ok(result) if result.detached.is_some() => {
            let detached = result.detached.expect("detached result");
            Ok(format_detached_output(snapshot, detached))
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
                usage: None,
                added_tool_names: None,
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
                    &format!("Command timed out after {timeout_secs} seconds"),
                )));
            }
            Err(error)
        }
    }
}

fn finish_output(
    output: &Arc<Mutex<OutputAccumulator>>,
    throttle: &Arc<UpdateThrottle>,
) -> super::output_accumulator::OutputSnapshot {
    let mut accumulator = output.lock().expect("output lock");
    accumulator.finish();
    let snapshot = accumulator.snapshot(SnapshotOptions {
        persist_if_truncated: true,
    });
    throttle.emit_snapshot(&snapshot);
    let _ = accumulator.close_temp_file();
    snapshot
}

fn format_output(
    output: &Arc<Mutex<OutputAccumulator>>,
    snapshot: &super::output_accumulator::OutputSnapshot,
    empty_text: &str,
) -> (String, Option<Value>) {
    let truncation = &snapshot.truncation;
    let mut text = if snapshot.content.is_empty() {
        empty_text.to_owned()
    } else {
        snapshot.content.clone()
    };
    let mut details = Map::new();
    if truncation.truncated {
        details.insert(
            "truncation".to_owned(),
            serde_json::to_value(truncation).expect("truncation metadata must serialize"),
        );
        if let Some(path) = &snapshot.full_output_path {
            details.insert("fullOutputPath".to_owned(), Value::String(path.clone()));
        }
        let start_line = truncation.total_lines - truncation.output_lines + 1;
        let end_line = truncation.total_lines;
        let full_output_path = snapshot.full_output_path.clone().unwrap_or_default();
        if truncation.last_line_partial {
            let last_line_bytes = output.lock().expect("output lock").last_line_bytes();
            text.push_str(&format!(
                "\n\n[Showing last {} of line {end_line} (line is {}). Full output: {full_output_path}]",
                format_size(truncation.output_bytes),
                format_size(last_line_bytes)
            ));
        } else if truncation.truncated_by == Some(TruncatedBy::Lines) {
            text.push_str(&format!(
                "\n\n[Showing lines {start_line}-{end_line} of {}. Full output: {full_output_path}]",
                truncation.total_lines
            ));
        } else {
            text.push_str(&format!(
                "\n\n[Showing lines {start_line}-{end_line} of {} ({} limit). Full output: {full_output_path}]",
                truncation.total_lines,
                format_size(DEFAULT_MAX_BYTES)
            ));
        }
    }
    (text, (!details.is_empty()).then_some(Value::Object(details)))
}

fn append_status(text: &str, status: &str) -> String {
    if text.is_empty() {
        status.to_owned()
    } else {
        format!("{text}\n\n{status}")
    }
}

fn format_detached_output(
    snapshot: super::output_accumulator::OutputSnapshot,
    detached: BashDetachedResult,
) -> AgentToolResult {
    let text = snapshot.content.trim_end().to_owned();
    let mut details = Map::new();
    details.insert("backgroundJobId".to_owned(), Value::String(detached.job_id.clone()));
    if let Some(log_path) = &detached.log_path {
        details.insert("fullOutputPath".to_owned(), Value::String(log_path.clone()));
    }
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(append_status(
            &text,
            &format!(
                "Command moved to background as job {}. {}",
                detached.job_id, detached.message
            ),
        )))],
        details: Some(Value::Object(details)),
        usage: None,
        added_tool_names: None,
        is_error: None,
        terminate: None,
    }
}

struct UpdateThrottle {
    on_update: AgentToolUpdateCallback,
    last_update_at: Mutex<Instant>,
    timer_running: AtomicBool,
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
        }
    }

    fn emit_initial(&self) {
        (self.on_update)(AgentToolResult {
            content: Vec::new(),
            details: None,
            usage: None,
            added_tool_names: None,
            is_error: None,
            terminate: None,
        });
    }

    fn schedule(self: &Arc<Self>, output: &Arc<Mutex<OutputAccumulator>>) {
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
                "truncation".to_owned(),
                serde_json::to_value(&snapshot.truncation).expect("truncation metadata must serialize"),
            );
        }
        if let Some(path) = &snapshot.full_output_path {
            details.insert("fullOutputPath".to_owned(), Value::String(path.clone()));
        }
        (self.on_update)(AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(snapshot.content.clone()))],
            details: Some(Value::Object(details)),
            usage: None,
            added_tool_names: None,
            is_error: None,
            terminate: None,
        });
    }
}

// ============================================================================
// Renderers
// ============================================================================

fn format_js_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn format_bash_call(args: &Value, theme: &dyn ExtensionTheme) -> String {
    let command = str_value(args.get("command"));
    let timeout = args.get("timeout").and_then(Value::as_f64);
    let timeout_suffix = match timeout {
        Some(timeout) if timeout != 0.0 => theme.fg("muted", &format!(" (timeout {}s)", format_js_number(timeout))),
        _ => String::new(),
    };
    let command_display = match command {
        None => invalid_arg_text(theme),
        Some(command) if command.is_empty() => theme.fg("toolOutput", "..."),
        Some(command) => command,
    };
    format!(
        "{}{timeout_suffix}",
        theme.fg("toolTitle", &theme.bold(&format!("$ {command_display}")))
    )
}

/// `renderCall` for the bash tool.
pub fn render_call(args: &Value, theme: &dyn ExtensionTheme, _context: &ToolRenderContext) -> ComponentRef {
    Rc::new(RefCell::new(Text::new(format_bash_call(args, theme), 0, 0)))
}

/// `createBashToolDefinition`: the bash tool plus its call renderer.
///
/// `renderResult` remains a gap: the collapsed preview needs
/// `components/visual-truncate.ts`.
pub fn create_bash_tool_definition(cwd: &str, options: BashToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(BashTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.render_call = Some(Arc::new(render_call));
    definition
}

/// `createBashTool`: the bash tool wrapped with the shared result cap.
pub fn create_bash_tool(cwd: &str, options: BashToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(BashTool::with_options(cwd, options)))
}
