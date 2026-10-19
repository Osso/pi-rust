//! Ported from `packages/coding-agent/src/core/tools/ls.ts`.
//!
//! Directory listing with the optional `rtk ls` fast path and byte
//! truncation. Renderers are pending `theme.ts`.

use std::io;
use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::collator::locale_compare;
use pi_tui::Text;
use pi_tui::tui::ComponentRef;
use serde_json::{Map, Value, json};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use super::path_utils::{path_exists, resolve_to_cwd};
use super::render_utils::{get_text_output, render_tool_path, str_value};
use super::tool_definition_wrapper::create_tool_definition_from_agent_tool;
use super::tool_definition_wrapper::wrap_agent_tool;
use super::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, TruncationResult, format_size, truncate_head};
use crate::core::extensions::types::{ToolDefinition, ToolRenderContext, ToolRenderResultOptions};
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{Theme, ThemeColor};
use std::cell::RefCell;
use std::rc::Rc;

const DESCRIPTION_PREFIX: &str =
    "List directory contents. Returns token-optimized entries, includes dotfiles, and truncates output to";
const DEFAULT_LIMIT: usize = 500;
const MAX_SAFE_INTEGER: usize = 9_007_199_254_740_991;
const RTK_MAX_BUFFER: usize = DEFAULT_MAX_BYTES * 2;

/// Directory access used by [`LsTool`]. Override to delegate listing to a remote system.
pub trait LsOperations: Send + Sync {
    fn exists<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, bool>;
    fn is_directory<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<bool>>;
    fn readdir<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<Vec<String>>>;
}

/// Local filesystem operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalLsOperations;

impl LsOperations for LocalLsOperations {
    fn exists<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, bool> {
        Box::pin(async move { path_exists(absolute_path).await })
    }

    fn is_directory<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            tokio::fs::metadata(absolute_path)
                .await
                .map(|metadata| metadata.is_dir())
        })
    }

    fn readdir<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<Vec<String>>> {
        Box::pin(async move {
            let mut entries = tokio::fs::read_dir(absolute_path).await?;
            let mut names = Vec::new();
            while let Some(entry) = entries.next_entry().await? {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
            Ok(names)
        })
    }
}

/// Options for [`LsTool`].
#[derive(Clone, Default)]
pub struct LsToolOptions {
    /// Custom directory access. When set, the `rtk ls` fast path is disabled.
    pub operations: Option<Arc<dyn LsOperations>>,
}

/// Lists directory entries.
pub struct LsTool {
    cwd: String,
    parameters: Value,
    operations: Arc<dyn LsOperations>,
    can_use_rtk: bool,
}

impl LsTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_options(cwd, LsToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<String>, options: LsToolOptions) -> Self {
        let can_use_rtk = options.operations.is_none();
        Self {
            cwd: cwd.into(),
            parameters: ls_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalLsOperations)),
            can_use_rtk,
        }
    }
}

fn ls_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "Directory to list (default: current directory)"},
            "limit": {"type": "number", "description": "Maximum number of entries to return (default: 500)"}
        }
    })
}

fn description() -> String {
    format!(
        "{DESCRIPTION_PREFIX} {DEFAULT_LIMIT} entries or {}KB (whichever is hit first).",
        DEFAULT_MAX_BYTES / 1024
    )
}

impl AgentTool for LsTool {
    fn name(&self) -> &str {
        "ls"
    }

    fn description(&self) -> &str {
        DESCRIPTION.as_str()
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "ls"
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
        let operations = Arc::clone(&self.operations);
        let can_use_rtk = self.can_use_rtk;
        Box::pin(async move { execute_ls(&cwd, operations, can_use_rtk, params, signal).await })
    }
}

static DESCRIPTION: std::sync::LazyLock<String> = std::sync::LazyLock::new(description);

async fn execute_ls(
    cwd: &str,
    operations: Arc<dyn LsOperations>,
    can_use_rtk: bool,
    params: Value,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    check_aborted(signal.as_ref())?;
    let path = params.get("path").and_then(Value::as_str).map(str::to_owned);
    let limit = params.get("limit").and_then(Value::as_u64).map(|value| value as usize);
    if can_use_rtk && let Some(result) = execute_rtk_ls(cwd, path.as_deref(), limit, signal.as_ref()).await? {
        return Ok(result);
    }
    list_with_operations(operations.as_ref(), cwd, path.as_deref(), limit, signal.as_ref()).await
}

async fn list_with_operations(
    operations: &dyn LsOperations,
    cwd: &str,
    path: Option<&str>,
    limit: Option<usize>,
    signal: Option<&CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let dir_path = resolve_to_cwd(path.unwrap_or("."), cwd).map_err(|error| ToolError::new(error.to_string()))?;
    let effective_limit = limit.unwrap_or(DEFAULT_LIMIT);
    if !operations.exists(&dir_path).await {
        return Err(ToolError::new(format!("Path not found: {dir_path}")));
    }
    let is_directory = operations
        .is_directory(&dir_path)
        .await
        .map_err(|error| ToolError::new(super::node_fs::node_fs_error(&error, "stat", &dir_path)))?;
    if !is_directory {
        return Err(ToolError::new(format!("Not a directory: {dir_path}")));
    }
    let mut entries = operations
        .readdir(&dir_path)
        .await
        .map_err(|error| ToolError::new(format!("Cannot read directory: {}", error)))?;
    entries.sort_by(|left, right| locale_compare(&left.to_lowercase(), &right.to_lowercase()));
    check_aborted(signal)?;

    let mut results = Vec::new();
    let mut entry_limit_reached = false;
    for entry in &entries {
        if results.len() >= effective_limit {
            entry_limit_reached = true;
            break;
        }
        let full_path = std::path::Path::new(&dir_path).join(entry);
        let full_path = full_path.to_string_lossy().into_owned();
        match operations.is_directory(&full_path).await {
            Ok(is_directory) => {
                results.push(if is_directory {
                    format!("{entry}/")
                } else {
                    entry.clone()
                });
            }
            Err(_) => continue,
        }
    }

    if results.is_empty() {
        return Ok(text_result("(empty directory)".to_owned(), None));
    }
    let raw_output = results.join("\n");
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
    if entry_limit_reached {
        notices.push(format!(
            "{effective_limit} entries limit reached. Use limit={} for more",
            effective_limit * 2
        ));
        details.insert("entryLimitReached".to_owned(), json!(effective_limit));
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
    Ok(text_result(
        output,
        (!details.is_empty()).then_some(Value::Object(details)),
    ))
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

async fn execute_rtk_ls(
    cwd: &str,
    path: Option<&str>,
    limit: Option<usize>,
    signal: Option<&CancellationToken>,
) -> Result<Option<AgentToolResult>, ToolError> {
    match run_rtk_ls(cwd, path.unwrap_or("."), signal).await {
        Ok(stdout) => Ok(Some(format_rtk_ls_output(&stdout, limit))),
        Err(RtkError::MissingExecutable) => Ok(None),
        Err(RtkError::Failed {
            message,
            stdout,
            stderr,
        }) => Err(ToolError::new(format_rtk_ls_error(&message, &stdout, &stderr))),
    }
}

enum RtkError {
    MissingExecutable,
    Failed {
        message: String,
        stdout: String,
        stderr: String,
    },
}

fn format_rtk_ls_error(message: &str, stdout: &str, stderr: &str) -> String {
    let stderr = stderr.trim();
    let stdout = stdout.trim();
    if !stderr.is_empty() {
        stderr.to_owned()
    } else if !stdout.is_empty() {
        stdout.to_owned()
    } else {
        message.to_owned()
    }
}

async fn run_rtk_ls(cwd: &str, path: &str, signal: Option<&CancellationToken>) -> Result<String, RtkError> {
    let mut command = tokio::process::Command::new("rtk");
    command
        .arg("ls")
        .arg(path)
        .current_dir(cwd)
        .env_clear()
        .envs(pi_js_compat::process_env::vars())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            RtkError::MissingExecutable
        } else {
            RtkError::Failed {
                message: format!("spawn rtk ENOENT: {error}"),
                stdout: String::new(),
                stderr: String::new(),
            }
        }
    })?;
    let mut stdout = child.stdout.take().expect("rtk stdout");
    let mut stderr = child.stderr.take().expect("rtk stderr");
    let stdout_task = tokio::spawn(async move {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer).await;
        buffer
    });
    let stderr_task = tokio::spawn(async move {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer).await;
        buffer
    });

    let status = wait_with_cancellation(&mut child, signal).await;
    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    let stdout = String::from_utf8_lossy(&stdout).into_owned();
    let stderr = String::from_utf8_lossy(&stderr).into_owned();
    if stdout.len() > RTK_MAX_BUFFER {
        return Err(RtkError::Failed {
            message: "stdout maxBuffer length exceeded".to_owned(),
            stdout: stdout[..RTK_MAX_BUFFER].to_owned(),
            stderr,
        });
    }
    match status {
        Ok(Some(status)) if status.success() => Ok(stdout),
        Ok(Some(status)) => Err(RtkError::Failed {
            message: format!("Command failed: rtk ls {} {}", path, status),
            stdout,
            stderr,
        }),
        Ok(None) => Err(RtkError::Failed {
            message: "The operation was aborted".to_owned(),
            stdout,
            stderr,
        }),
        Err(error) => Err(RtkError::Failed {
            message: error.to_string(),
            stdout,
            stderr,
        }),
    }
}

async fn wait_with_cancellation(
    child: &mut tokio::process::Child,
    signal: Option<&CancellationToken>,
) -> io::Result<Option<std::process::ExitStatus>> {
    let Some(signal) = signal else {
        return child.wait().await.map(Some);
    };
    tokio::select! {
        status = child.wait() => status.map(Some),
        () = signal.cancelled() => {
            let _ = child.kill().await;
            Ok(None)
        }
    }
}

fn format_rtk_ls_output(stdout: &str, limit: Option<usize>) -> AgentToolResult {
    let effective_limit = limit.unwrap_or(DEFAULT_LIMIT);
    let lines = stdout
        .trim_end()
        .split('\n')
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let limited_lines = lines.iter().take(effective_limit).copied().collect::<Vec<_>>();
    let entry_limit_reached = lines.len() > effective_limit;
    let raw_output = if limited_lines.is_empty() {
        "(empty directory)".to_owned()
    } else {
        limited_lines.join("\n")
    };
    let truncation = truncate_head(
        &raw_output,
        TruncationOptions {
            max_lines: MAX_SAFE_INTEGER,
            max_bytes: DEFAULT_MAX_BYTES,
        },
    );
    let mut details = Map::new();
    let mut notices = Vec::new();
    if entry_limit_reached {
        notices.push(format!(
            "{effective_limit} entries limit reached. Use limit={} for more",
            effective_limit * 2
        ));
        details.insert("entryLimitReached".to_owned(), json!(effective_limit));
    }
    if truncation.truncated {
        notices.push(format!("{} limit reached", format_size(DEFAULT_MAX_BYTES)));
        details.insert(
            "truncation".to_owned(),
            serde_json::to_value(&truncation).expect("truncation metadata must serialize"),
        );
    }
    let notice_text = if notices.is_empty() {
        String::new()
    } else {
        format!("\n\n[{}]", notices.join(". "))
    };
    text_result(
        format!("{}{notice_text}", truncation.content),
        (!details.is_empty()).then_some(Value::Object(details)),
    )
}

fn check_aborted(signal: Option<&CancellationToken>) -> Result<(), ToolError> {
    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Err(ToolError::new("Operation aborted"));
    }
    Ok(())
}

// ============================================================================
// Renderers
// ============================================================================

fn format_ls_call(args: &Value, theme: &Theme, cwd: &str) -> String {
    let path_display = render_tool_path(str_value(args.get("path")).as_deref(), theme, cwd, Some("."));
    let mut text = format!("{} {path_display}", theme.fg(ThemeColor::ToolTitle, &theme.bold("ls")));
    if let Some(limit) = args.get("limit").and_then(Value::as_f64) {
        text.push_str(&theme.fg(ThemeColor::ToolOutput, &format!(" (limit {})", format_js_number(limit))));
    }
    text
}

fn format_js_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn format_ls_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    show_images: bool,
) -> String {
    let output = get_text_output(Some(result), show_images).trim().to_owned();
    let mut text = String::new();
    if !output.is_empty() {
        let lines = output.split('\n').collect::<Vec<_>>();
        let max_lines = if options.expanded { lines.len() } else { 20 };
        let display_lines = lines.iter().take(max_lines).collect::<Vec<_>>();
        let remaining = lines.len().saturating_sub(max_lines);
        text.push_str(&format!(
            "\n{}",
            display_lines
                .iter()
                .map(|line| theme.fg(ThemeColor::ToolOutput, line))
                .collect::<Vec<_>>()
                .join("\n")
        ));
        if remaining > 0 {
            text.push_str(&format!(
                "{}{}{}",
                theme.fg(ThemeColor::Muted, &format!("\n... ({remaining} more lines,")),
                key_hint("app.tools.expand", "to expand"),
                theme.fg(ThemeColor::Muted, ")")
            ));
        }
    }
    let entry_limit = result
        .details
        .as_ref()
        .and_then(|details| details.get("entryLimitReached"))
        .and_then(Value::as_u64);
    let truncation = result
        .details
        .as_ref()
        .and_then(|details| details.get("truncation"))
        .and_then(|value| serde_json::from_value::<TruncationResult>(value.clone()).ok());
    if entry_limit.is_some() || truncation.as_ref().is_some_and(|truncation| truncation.truncated) {
        let mut warnings = Vec::new();
        if let Some(entry_limit) = entry_limit {
            warnings.push(format!("{entry_limit} entries limit"));
        }
        if let Some(truncation) = truncation.filter(|truncation| truncation.truncated) {
            warnings.push(format!("{} limit", format_size(truncation.max_bytes)));
        }
        text.push_str(&format!(
            "\n{}",
            theme.fg(ThemeColor::Warning, &format!("[Truncated: {}]", warnings.join(", ")))
        ));
    }
    text
}

/// `renderCall` for the ls tool.
pub fn render_call(args: &Value, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    Rc::new(RefCell::new(Text::new(format_ls_call(args, theme, &context.cwd), 0, 0)))
}

/// `renderResult` for the ls tool.
pub fn render_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    Rc::new(RefCell::new(Text::new(
        format_ls_result(result, options, theme, context.show_images),
        0,
        0,
    )))
}

/// `createLsToolDefinition`: the ls tool plus its renderers.
pub fn create_ls_tool_definition(cwd: &str, options: LsToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(LsTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.render_call = Some(Arc::new(render_call));
    definition.render_result = Some(Arc::new(render_result));
    definition
}

/// `createLsTool`: the ls tool wrapped with the shared result cap.
pub fn create_ls_tool(cwd: &str, options: LsToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(LsTool::with_options(cwd, options)))
}
