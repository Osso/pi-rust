//! Ported from `packages/coding-agent/src/core/tools/find.ts`.
//!
//! Runs the real `fd` binary with the source argv, relativizes results, and
//! applies the source limits. Renderers are pending `theme.ts`.

use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::{Map, Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio_util::sync::CancellationToken;

use super::node_fs::node_error_code;
use super::path_utils::{path_exists, resolve_to_cwd};
use super::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, format_size, truncate_head};
use crate::utils::tools_manager::ToolsManager;

const DESCRIPTION_PREFIX: &str = "Search for files by glob pattern. Returns matching file paths relative to the search directory. Respects .gitignore. Output is truncated to";
const DEFAULT_LIMIT: usize = 1000;
const MAX_SAFE_INTEGER: usize = 9_007_199_254_740_991;

/// Filesystem access used by [`FindTool`]. Override to delegate searches to a remote system.
pub trait FindOperations: Send + Sync {
    fn exists<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, bool>;

    /// Whether [`FindOperations::glob`] is implemented.
    fn supports_glob(&self) -> bool {
        false
    }

    /// Custom glob implementation; only called when [`FindOperations::supports_glob`] is true.
    fn glob<'a>(
        &'a self,
        pattern: &'a str,
        cwd: &'a str,
        ignore: &'a [String],
        limit: usize,
    ) -> BoxFuture<'a, Vec<String>> {
        let _ = (pattern, cwd, ignore, limit);
        Box::pin(async { Vec::new() })
    }
}

/// Local filesystem operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalFindOperations;

impl FindOperations for LocalFindOperations {
    fn exists<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, bool> {
        Box::pin(async move { path_exists(absolute_path).await })
    }
}

/// Options for [`FindTool`].
#[derive(Clone, Default)]
pub struct FindToolOptions {
    /// Custom filesystem access. Default: local filesystem plus fd.
    pub operations: Option<Arc<dyn FindOperations>>,
}

/// Finds files by glob pattern.
pub struct FindTool {
    cwd: String,
    parameters: Value,
    operations: Option<Arc<dyn FindOperations>>,
}

impl FindTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_options(cwd, FindToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<String>, options: FindToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: find_schema(),
            operations: options.operations,
        }
    }
}

fn find_schema() -> Value {
    json!({
        "type": "object",
        "required": ["pattern"],
        "properties": {
            "pattern": {"type": "string", "description": "Glob pattern to match files, e.g. '*.ts', '**/*.json', or 'src/**/*.spec.ts'"},
            "path": {"type": "string", "description": "Directory to search in (default: current directory)"},
            "limit": {"type": "number", "description": "Maximum number of results (default: 1000)"}
        }
    })
}

static DESCRIPTION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "{DESCRIPTION_PREFIX} {DEFAULT_LIMIT} results or {}KB (whichever is hit first).",
        DEFAULT_MAX_BYTES / 1024
    )
});

impl AgentTool for FindTool {
    fn name(&self) -> &str {
        "find"
    }

    fn description(&self) -> &str {
        DESCRIPTION.as_str()
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "find"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let cwd = self.cwd.clone();
        let operations = self.operations.clone();
        Box::pin(async move { execute_find(&cwd, operations, params, signal).await })
    }
}

struct FindInput {
    pattern: String,
    search_dir: Option<String>,
    limit: Option<usize>,
}

fn parse_find_input(params: &Value) -> Result<FindInput, ToolError> {
    let pattern = params
        .get("pattern")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("find tool requires a string pattern"))?
        .to_owned();
    Ok(FindInput {
        pattern,
        search_dir: params.get("path").and_then(Value::as_str).map(str::to_owned),
        limit: params
            .get("limit")
            .and_then(Value::as_f64)
            .map(|value| value.trunc().max(0.0) as usize),
    })
}

async fn execute_find(
    cwd: &str,
    operations: Option<Arc<dyn FindOperations>>,
    params: Value,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let input = parse_find_input(&params)?;
    check_aborted(signal.as_ref())?;
    let search_path = resolve_to_cwd(input.search_dir.as_deref().unwrap_or("."), cwd)
        .map_err(|error| ToolError::new(error.to_string()))?;
    let effective_limit = input.limit.unwrap_or(DEFAULT_LIMIT);

    if let Some(custom) = operations.as_deref().filter(|operations| operations.supports_glob()) {
        if !custom.exists(&search_path).await {
            return Err(ToolError::new(format!("Path not found: {search_path}")));
        }
        check_aborted(signal.as_ref())?;
        let results = custom
            .glob(
                &input.pattern,
                &search_path,
                &["**/node_modules/**".to_owned(), "**/.git/**".to_owned()],
                effective_limit,
            )
            .await;
        check_aborted(signal.as_ref())?;
        if results.is_empty() {
            return Ok(text_result("No files found matching pattern".to_owned(), None));
        }
        let relativized = results
            .iter()
            .map(|path| relativize_find_result_path(path, &search_path))
            .collect::<Vec<_>>();
        let result_limit_reached = relativized.len() >= effective_limit;
        let (result_output, details) = relativize_output(relativized, effective_limit, result_limit_reached, false);
        return Ok(text_result(result_output, details));
    }

    let mut tools = ToolsManager::for_process(pi_ai::http::default_fetch());
    let fd_path = tools.ensure_tool("fd").await;
    check_aborted(signal.as_ref())?;
    let Some(fd_path) = fd_path else {
        return Err(ToolError::new("fd is not available on PATH"));
    };

    let mut args = vec!["--glob".to_owned(), "--color=never".to_owned(), "--hidden".to_owned()];
    let inside_git_repo = inside_git_repo(&search_path).await;
    if !inside_git_repo {
        args.push("--no-require-git".to_owned());
    }
    args.push("--max-results".to_owned());
    args.push(effective_limit.to_string());

    let mut effective_pattern = input.pattern.clone();
    if input.pattern.contains('/') {
        args.push("--full-path".to_owned());
        if !input.pattern.starts_with('/') && !input.pattern.starts_with("**/") && input.pattern != "**" {
            effective_pattern = format!("**/{}", input.pattern);
        }
        if cfg!(windows) {
            effective_pattern = effective_pattern.replace('/', "[/\\\\]");
        }
    }
    args.push("--".to_owned());
    args.push(effective_pattern);
    args.push(search_path.clone());

    let output = run_fd(&fd_path, &args, signal.as_ref()).await?;
    if output.aborted {
        return Err(ToolError::new("Operation aborted"));
    }
    let raw_output = output.lines.join("\n");
    if raw_output.is_empty() {
        if let Some(failure) = output.failure {
            return Err(ToolError::new(failure));
        }
        return Ok(text_result("No files found matching pattern".to_owned(), None));
    }

    let relativized = output
        .lines
        .iter()
        .map(|line| line.trim_end_matches('\r').trim())
        .filter(|line| !line.is_empty())
        .map(|line| relativize_find_result_path(line, &search_path))
        .collect::<Vec<_>>();
    let result_limit_reached = relativized.len() >= effective_limit;
    let (result_output, details) = relativize_output(relativized, effective_limit, result_limit_reached, true);
    Ok(text_result(result_output, details))
}

/// Relativizes a find result against the search root and normalizes separators.
pub fn relativize_find_result_path(result_path: &str, search_path: &str) -> String {
    let had_trailing_separator = result_path.ends_with('/') || (cfg!(windows) && result_path.ends_with('\\'));
    let relative = if std::path::Path::new(result_path).is_absolute() {
        crate::utils::paths::relative_path(search_path, result_path)
    } else {
        result_path.to_owned()
    };
    let posix_path = relative.replace('\\', "/");
    if had_trailing_separator && !posix_path.ends_with('/') {
        format!("{posix_path}/")
    } else {
        posix_path
    }
}

fn relativize_output(
    relativized: Vec<String>,
    effective_limit: usize,
    result_limit_reached: bool,
    include_refine_hint: bool,
) -> (String, Option<Value>) {
    let raw_output = relativized.join("\n");
    let truncation = truncate_head(
        &raw_output,
        TruncationOptions {
            max_lines: MAX_SAFE_INTEGER,
            max_bytes: DEFAULT_MAX_BYTES,
        },
    );
    let mut output = truncation.content.clone();
    let mut details = Map::new();
    let mut notices = Vec::new();
    if result_limit_reached {
        notices.push(if include_refine_hint {
            format!(
                "{effective_limit} results limit reached. Use limit={} for more, or refine pattern",
                effective_limit * 2
            )
        } else {
            format!("{effective_limit} results limit reached")
        });
        details.insert("resultLimitReached".to_owned(), json!(effective_limit));
    }
    if truncation.truncated {
        notices.push(format!("{} limit reached", format_size(DEFAULT_MAX_BYTES)));
        details.insert(
            "truncation".to_owned(),
            serde_json::to_value(&truncation).expect("truncation metadata must serialize"),
        );
    }
    if !notices.is_empty() {
        output.push_str(&format!("\n\n[{}]", notices.join(". ")));
    }
    (output, (!details.is_empty()).then_some(Value::Object(details)))
}

async fn inside_git_repo(search_path: &str) -> bool {
    let mut current = std::path::PathBuf::from(search_path);
    loop {
        if path_exists(&current.join(".git").to_string_lossy()).await {
            return true;
        }
        let Some(parent) = current.parent() else {
            return false;
        };
        if parent == current {
            return false;
        }
        current = parent.to_path_buf();
    }
}

struct FdOutput {
    lines: Vec<String>,
    failure: Option<String>,
    aborted: bool,
}

async fn run_fd(fd_path: &str, args: &[String], signal: Option<&CancellationToken>) -> Result<FdOutput, ToolError> {
    let mut command = tokio::process::Command::new(fd_path);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| ToolError::new(format!("Failed to run fd: spawn {fd_path} {}", node_error_code(&error))))?;
    let stdout = child.stdout.take().expect("fd stdout");
    let mut stderr = child.stderr.take().expect("fd stderr");
    let stderr_task = tokio::spawn(async move {
        let mut buffer = String::new();
        let _ = stderr.read_to_string(&mut buffer).await;
        buffer
    });
    let mut lines = Vec::new();
    let mut reader = BufReader::new(stdout).lines();
    let mut aborted = false;
    loop {
        tokio::select! {
            line = reader.next_line() => {
                let Ok(Some(line)) = line else { break };
                lines.push(line);
            }
            () = cancellation(signal) => {
                aborted = true;
                let _ = child.start_kill();
                let _ = child.wait().await;
                break;
            }
        }
    }
    let status = child.wait().await.map_err(|error| ToolError::new(error.to_string()))?;
    let stderr = stderr_task.await.unwrap_or_default();
    let failure = if status.code() != Some(0) {
        let message = stderr.trim();
        let message = if message.is_empty() {
            format!(
                "fd exited with code {}",
                status.code().map_or("null".to_owned(), |code| code.to_string())
            )
        } else {
            message.to_owned()
        };
        Some(message)
    } else {
        None
    };
    Ok(FdOutput {
        lines,
        failure,
        aborted,
    })
}

async fn cancellation(signal: Option<&CancellationToken>) {
    match signal {
        Some(signal) => signal.cancelled().await,
        None => std::future::pending().await,
    }
}

fn text_result(text: String, details: Option<Value>) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details,
        usage: None,
        added_tool_names: None,
        is_error: None,
        terminate: None,
    }
}

fn check_aborted(signal: Option<&CancellationToken>) -> Result<(), ToolError> {
    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Err(ToolError::new("Operation aborted"));
    }
    Ok(())
}
