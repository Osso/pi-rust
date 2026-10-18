use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

use pi_ai::request::BoxFuture;
use pi_ai::request::OnRetry;
use pi_ai::types::{CacheRetention, Transport};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::agent::{AgentToolResult, AgentToolUpdateCallback, ToolError};

/// Skill loaded from a `SKILL.md` file or provided by an application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub file_path: String,
    pub disable_model_invocation: Option<bool>,
}

/// Prompt template that can be formatted into a prompt for explicit invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplate {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
}

/// Resources made available to explicit invocation methods and system-prompt callbacks.
#[derive(Debug, Clone)]
pub struct AgentHarnessResources {
    pub prompt_templates: Option<Vec<PromptTemplate>>,
    pub skills: Option<Vec<Skill>>,
}

/// Static tool context or zero-argument provider resolved for each turn snapshot.
pub enum AgentHarnessToolContextSource<TContext> {
    Static(TContext),
    Provider(Arc<dyn Fn() -> BoxFuture<'static, TContext> + Send + Sync>),
}

/// Tool definition executed by a harness with an application-defined context.
pub trait AgentHarnessTool<TContext>: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    /// JSON Schema for the tool arguments.
    fn parameters(&self) -> &Value;
    /// Human-readable label for UI display.
    fn label(&self) -> &str;
    /// Per-tool execution mode override.
    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        None
    }
    /// Compatibility shim for raw tool-call arguments before schema validation.
    fn prepare_arguments(&self, args: Value) -> Value {
        args
    }
    /// Prepare durable execution metadata before the assistant tool call is persisted.
    fn prepare_execution(
        &self,
        _tool_call_id: String,
        _signal: Option<CancellationToken>,
    ) -> BoxFuture<'static, Option<pi_ai::types::ToolCallExecution>> {
        Box::pin(async { None })
    }
    /// Execute the tool call with the context resolved for the current turn snapshot.
    fn execute(
        &self,
        tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        on_update: Option<AgentToolUpdateCallback>,
        context: TContext,
        execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>>;
}

/// Curated provider request options owned by the harness and snapshotted per turn.
#[derive(Clone, Default)]
pub struct AgentHarnessStreamOptions {
    pub transport: Option<Transport>,
    pub timeout_ms: Option<f64>,
    pub websocket_connect_timeout_ms: Option<f64>,
    pub max_retries: Option<f64>,
    pub max_retry_delay_ms: Option<f64>,
    pub headers: Option<BTreeMap<String, String>>,
    pub metadata: Option<Map<String, Value>>,
    pub cache_retention: Option<CacheRetention>,
    pub on_retry: Option<OnRetry>,
}

/// Per-request option patch: `Unchanged` keeps the current value, `Clear` removes it entirely.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum OptionalPatch<T> {
    #[default]
    Unchanged,
    Clear,
    Set(T),
}

/// Per-request stream option patch returned by provider hooks.
///
/// Header and metadata patches delete keys mapped to `None`; `Clear` removes all entries.
#[derive(Clone, Default)]
pub struct AgentHarnessStreamOptionsPatch {
    pub transport: Option<Transport>,
    pub timeout_ms: Option<f64>,
    pub websocket_connect_timeout_ms: Option<f64>,
    pub max_retries: Option<f64>,
    pub max_retry_delay_ms: Option<f64>,
    pub headers: OptionalPatch<BTreeMap<String, Option<String>>>,
    pub metadata: OptionalPatch<BTreeMap<String, Option<Value>>>,
    pub cache_retention: Option<CacheRetention>,
    pub on_retry: Option<OnRetry>,
}

/// Controls how tool calls from one assistant message are executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionMode {
    Sequential,
    Parallel,
}

impl ToolExecutionMode {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sequential => "sequential",
            Self::Parallel => "parallel",
        }
    }
}

/// Controls how queued messages are drained at a queue point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueMode {
    All,
    OneAtATime,
}

impl QueueMode {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::OneAtATime => "one-at-a-time",
        }
    }
}

/// Thinking/reasoning level for models that support it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
    Ultra,
}

impl ThinkingLevel {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
        }
    }
}

/// Context supplied while an agent tool call is executing.
#[derive(Debug)]
pub struct AgentToolExecutionContext {
    pub started_at: f64,
    pub agent_id: Option<String>,
}

/// Result returned by a before-tool-call hook.
#[derive(Debug)]
pub struct BeforeToolCallResult {
    pub block: Option<bool>,
    pub reason: Option<String>,
    pub terminate: Option<bool>,
}

/// Kind of filesystem object addressed by a `FileInfo` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    File,
    Directory,
    Symlink,
}

impl FileKind {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
        }
    }
}

/// Metadata for one filesystem object.
#[derive(Debug)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: f64,
    pub mtime_ms: f64,
}

/// Stable, backend-independent filesystem failure codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileErrorCode {
    Aborted,
    NotFound,
    PermissionDenied,
    NotDirectory,
    IsDirectory,
    Invalid,
    NotSupported,
    Unknown,
}

impl FileErrorCode {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aborted => "aborted",
            Self::NotFound => "not_found",
            Self::PermissionDenied => "permission_denied",
            Self::NotDirectory => "not_directory",
            Self::IsDirectory => "is_directory",
            Self::Invalid => "invalid",
            Self::NotSupported => "not_supported",
            Self::Unknown => "unknown",
        }
    }
}

/// Error returned by filesystem operations.
#[derive(Debug)]
pub struct FileError {
    pub name: String,
    pub message: String,
    pub code: FileErrorCode,
    pub path: Option<String>,
    pub cause: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl FileError {
    pub fn new(
        code: FileErrorCode,
        message: impl Into<String>,
        path: Option<String>,
        cause: Option<Box<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            name: "FileError".to_owned(),
            message: message.into(),
            code,
            path,
            cause,
        }
    }
}

impl Display for FileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for FileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_deref().map(|error| error as &(dyn Error + 'static))
    }
}

/// Stable, backend-independent process-execution failure codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionErrorCode {
    Aborted,
    Timeout,
    ShellUnavailable,
    SpawnError,
    CallbackError,
    Unknown,
}

impl ExecutionErrorCode {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aborted => "aborted",
            Self::Timeout => "timeout",
            Self::ShellUnavailable => "shell_unavailable",
            Self::SpawnError => "spawn_error",
            Self::CallbackError => "callback_error",
            Self::Unknown => "unknown",
        }
    }
}

/// Error returned by process-execution operations.
#[derive(Debug)]
pub struct ExecutionError {
    pub name: String,
    pub message: String,
    pub code: ExecutionErrorCode,
    pub cause: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl ExecutionError {
    pub fn new(
        code: ExecutionErrorCode,
        message: impl Into<String>,
        cause: Option<Box<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            name: "ExecutionError".to_owned(),
            message: message.into(),
            code,
            cause,
        }
    }
}

impl Display for ExecutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_deref().map(|error| error as &(dyn Error + 'static))
    }
}

/// Stable compaction failure codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionErrorCode {
    Aborted,
    SummarizationFailed,
}

impl CompactionErrorCode {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aborted => "aborted",
            Self::SummarizationFailed => "summarization_failed",
        }
    }
}

/// Error returned by compaction helpers.
#[derive(Debug)]
pub struct CompactionError {
    pub name: String,
    pub message: String,
    pub code: CompactionErrorCode,
    pub cause: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl CompactionError {
    pub fn new(
        code: CompactionErrorCode,
        message: impl Into<String>,
        cause: Option<Box<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            name: "CompactionError".to_owned(),
            message: message.into(),
            code,
            cause,
        }
    }
}

impl Display for CompactionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for CompactionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_deref().map(|error| error as &(dyn Error + 'static))
    }
}

/// Stable branch-summary failure codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchSummaryErrorCode {
    Aborted,
    SummarizationFailed,
}

impl BranchSummaryErrorCode {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aborted => "aborted",
            Self::SummarizationFailed => "summarization_failed",
        }
    }
}

/// Error returned by branch-summary helpers.
#[derive(Debug)]
pub struct BranchSummaryError {
    pub name: String,
    pub message: String,
    pub code: BranchSummaryErrorCode,
    pub cause: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl BranchSummaryError {
    pub fn new(
        code: BranchSummaryErrorCode,
        message: impl Into<String>,
        cause: Option<Box<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            name: "BranchSummaryError".to_owned(),
            message: message.into(),
            code,
            cause,
        }
    }
}

impl Display for BranchSummaryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for BranchSummaryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_deref().map(|error| error as &(dyn Error + 'static))
    }
}

/// Content accepted by filesystem write and append operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileContent<'a> {
    Text(&'a str),
    Bytes(&'a [u8]),
}

impl<'a> FileContent<'a> {
    pub fn as_bytes(self) -> &'a [u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Bytes(bytes) => bytes,
        }
    }
}

/// Options for [`FileSystem::read_text_lines`].
#[derive(Debug, Clone, Default)]
pub struct ReadTextLinesOptions {
    pub max_lines: Option<usize>,
    pub abort: Option<CancellationToken>,
}

/// Options for [`FileSystem::create_dir`]. `recursive` defaults to `true`.
#[derive(Debug, Clone, Default)]
pub struct CreateDirOptions {
    pub recursive: Option<bool>,
}

/// Options for [`FileSystem::remove`]. Both flags default to `false`.
#[derive(Debug, Clone, Default)]
pub struct RemoveOptions {
    pub recursive: Option<bool>,
    pub force: Option<bool>,
}

/// Options for [`FileSystem::create_temp_file`].
#[derive(Debug, Clone, Default)]
pub struct CreateTempFileOptions {
    pub prefix: Option<String>,
    pub suffix: Option<String>,
}

/// Shell stdout/stderr callback. `Err` carries the callback error message.
pub type ShellOutputHandler = dyn Fn(&str) -> Result<(), String> + Send + Sync;

/// Options for [`Shell::exec`].
#[derive(Clone, Default)]
pub struct ShellExecOptions {
    /// Working directory for the command; relative paths resolve against [`FileSystem::cwd`].
    pub cwd: Option<String>,
    /// Environment variables that override inherited defaults when `inherit_env` is `true`.
    pub env: Option<HashMap<String, String>>,
    /// Whether to inherit the execution environment's default variables. Defaults to `true`.
    pub inherit_env: Option<bool>,
    /// Timeout in seconds. No timeout when absent.
    pub timeout: Option<f64>,
    /// Cancellation token used to terminate the command.
    pub abort: Option<CancellationToken>,
    /// Called with stdout chunks as they are produced. `Err` carries the callback error message.
    pub on_stdout: Option<Arc<ShellOutputHandler>>,
    /// Called with stderr chunks as they are produced. `Err` carries the callback error message.
    pub on_stderr: Option<Arc<ShellOutputHandler>>,
}

impl std::fmt::Debug for ShellExecOptions {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShellExecOptions")
            .field("cwd", &self.cwd)
            .field("env", &self.env)
            .field("inherit_env", &self.inherit_env)
            .field("timeout", &self.timeout)
            .field("abort", &self.abort)
            .field("on_stdout", &self.on_stdout.as_ref().map(|_| "<callback>"))
            .field("on_stderr", &self.on_stderr.as_ref().map(|_| "<callback>"))
            .finish()
    }
}

/// Successful process-execution result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Filesystem capability used by the harness.
///
/// Paths passed to methods may be absolute or relative to [`FileSystem::cwd`]. Paths returned by
/// file operations are addressed paths in the filesystem namespace, but are not canonicalized
/// through symlinks unless returned by [`FileSystem::canonical_path`].
pub trait FileSystem: Send + Sync {
    /// Current working directory for relative paths.
    fn cwd(&self) -> &str;
    /// Return an absolute addressed path without requiring it to exist and without resolving symlinks.
    fn absolute_path<'a>(
        &'a self,
        path: &'a str,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<String, FileError>>;
    /// Join path segments in the filesystem namespace without requiring the result to exist.
    fn join_path<'a>(
        &'a self,
        parts: &'a [String],
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<String, FileError>>;
    fn read_text_file<'a>(
        &'a self,
        path: &'a str,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<String, FileError>>;
    fn read_text_lines<'a>(
        &'a self,
        path: &'a str,
        options: ReadTextLinesOptions,
    ) -> BoxFuture<'a, Result<Vec<String>, FileError>>;
    fn read_binary_file<'a>(
        &'a self,
        path: &'a str,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<Vec<u8>, FileError>>;
    fn write_file<'a>(
        &'a self,
        path: &'a str,
        content: FileContent<'a>,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<(), FileError>>;
    fn append_file<'a>(&'a self, path: &'a str, content: FileContent<'a>) -> BoxFuture<'a, Result<(), FileError>>;
    fn rename_file<'a>(
        &'a self,
        source_path: &'a str,
        destination_path: &'a str,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<(), FileError>>;
    fn file_info<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<FileInfo, FileError>>;
    fn list_dir<'a>(
        &'a self,
        path: &'a str,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<Vec<FileInfo>, FileError>>;
    fn canonical_path<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<String, FileError>>;
    fn exists<'a>(
        &'a self,
        path: &'a str,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<bool, FileError>>;
    fn create_dir<'a>(&'a self, path: &'a str, options: CreateDirOptions) -> BoxFuture<'a, Result<(), FileError>>;
    fn remove<'a>(&'a self, path: &'a str, options: RemoveOptions) -> BoxFuture<'a, Result<(), FileError>>;
    fn create_temp_dir<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<String, FileError>>;
    fn create_temp_file<'a>(&'a self, options: CreateTempFileOptions) -> BoxFuture<'a, Result<String, FileError>>;
    /// Release filesystem resources. Must be best-effort and must not fail.
    fn cleanup(&self) -> BoxFuture<'_, ()>;
}

/// Shell execution capability used by the harness.
pub trait Shell: Send + Sync {
    fn exec<'a>(
        &'a self,
        command: &'a str,
        options: ShellExecOptions,
    ) -> BoxFuture<'a, Result<ShellExecResult, ExecutionError>>;
    /// Release shell resources. Must be best-effort and must not fail.
    fn cleanup(&self) -> BoxFuture<'_, ()>;
}

/// Filesystem and process execution environment used by the harness.
pub trait ExecutionEnv: FileSystem + Shell {}

impl<T> ExecutionEnv for T where T: FileSystem + Shell {}
