//! Ported from `packages/coding-agent/src/core/tools/ls.ts`.
//!
//! Directory listing with the optional `rtk ls` fast path and byte
//! truncation. Renderers are pending `theme.ts`.

use pi_js_compat::js_json::JsValue;
use std::io;
use std::sync::Arc;

use crate::utils::paths::PathError;
use crate::utils::paths::node_join;
use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::collator::locale_compare_utf16;
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject as Map, JsString, JsValue as Value, encode_node_utf8};
use pi_tui::Text;
use pi_tui::tui::ComponentRef;
use serde_json::Value as SchemaValue;
use tokio::io::AsyncReadExt;

use super::path_utils::{path_exists, resolve_to_cwd};
use super::render_utils::{get_text_output, render_tool_path, str_value};
use super::tool_definition_wrapper::create_tool_definition_from_agent_tool;
use super::tool_definition_wrapper::wrap_agent_tool;
use super::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, TruncationResult, format_size, truncate_head};
use crate::core::extensions::types::{
    HandlerError, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_result,
};
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{Theme, ThemeColor};
use crate::utils::child_process::node_spawn_error_message;
use std::cell::RefCell;
use std::rc::Rc;

const DESCRIPTION_PREFIX: &str =
    "List directory contents. Returns token-optimized entries, includes dotfiles, and truncates output to";
const DEFAULT_LIMIT: usize = 500;
const MAX_SAFE_INTEGER: usize = 9_007_199_254_740_991;

/// `LS_TOOL_SYSTEM_PROMPT_CONTRIBUTION`.
pub const LS_TOOL_SYSTEM_PROMPT_CONTRIBUTION: super::ToolSystemPromptContribution =
    super::ToolSystemPromptContribution {
        snippet: "List directory contents",
        guidelines: &[],
    };
const RTK_MAX_BUFFER: usize = DEFAULT_MAX_BYTES * 2;

/// Directory access used by [`LsTool`]. Override to delegate listing to a remote system.
pub trait LsOperations: Send + Sync {
    fn exists<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<bool>>;
    fn is_directory<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<bool>>;
    fn readdir<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<JsString>>>;
}

/// Local filesystem operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalLsOperations;

impl LsOperations for LocalLsOperations {
    fn exists<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move { Ok(path_exists(absolute_path).await) })
    }

    fn is_directory<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            super::node_fs::validate_node_path(absolute_path)?;
            tokio::fs::metadata(&encode_node_utf8(absolute_path))
                .await
                .map(|metadata| metadata.is_dir())
                .map_err(|error| {
                    super::node_fs::operation_error(super::node_fs::node_fs_error(&error, "stat", &absolute_path))
                })
        })
    }

    fn readdir<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<JsString>>> {
        Box::pin(async move {
            super::node_fs::validate_node_path(absolute_path)?;
            let mut entries = tokio::fs::read_dir(encode_node_utf8(absolute_path)).await?;
            let mut names = Vec::new();
            while let Some(entry) = entries.next_entry().await? {
                names.push(entry.file_name().to_string_lossy().into_owned().into());
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
    cwd: JsString,
    parameters: SchemaValue,
    operations: Arc<dyn LsOperations>,
    can_use_rtk: bool,
}

impl LsTool {
    pub fn new(cwd: impl Into<JsString>) -> Self {
        Self::with_options(cwd, LsToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<JsString>, options: LsToolOptions) -> Self {
        let can_use_rtk = options.operations.is_none();
        Self {
            cwd: cwd.into(),
            parameters: ls_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalLsOperations)),
            can_use_rtk,
        }
    }
}

fn ls_schema() -> SchemaValue {
    serde_json::json!({
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

    fn description(&self) -> pi_js_compat::js_json::JsString {
        DESCRIPTION.as_str().into()
    }

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "ls".into()
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        signal: Option<AbortSignal>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let cwd = self.cwd.clone();
        let operations = Arc::clone(&self.operations);
        let can_use_rtk = self.can_use_rtk;
        Box::pin(async move {
            let result = crate::utils::abort::race_with_abort_signal(
                execute_ls(&cwd, operations, can_use_rtk, params, signal.clone()),
                signal.as_ref(),
            )
            .await;
            check_aborted(signal.as_ref())?;
            result.map_err(|_| ToolError::new("Operation aborted"))?
        })
    }
}

static DESCRIPTION: std::sync::LazyLock<String> = std::sync::LazyLock::new(description);

async fn execute_ls(
    cwd: &JsString,
    operations: Arc<dyn LsOperations>,
    can_use_rtk: bool,
    params: JsValue,
    signal: Option<AbortSignal>,
) -> Result<AgentToolResult, ToolError> {
    check_aborted(signal.as_ref())?;
    let path = params.get("path").and_then(Value::as_string);
    let limit = params.get("limit").and_then(Value::as_u64).map(|value| value as usize);
    if can_use_rtk && let Some(result) = execute_rtk_ls(cwd, path, limit, signal.as_ref()).await? {
        return Ok(result);
    }
    list_with_operations(operations.as_ref(), cwd, path, limit, signal.as_ref()).await
}

async fn list_with_operations(
    operations: &dyn LsOperations,
    cwd: &JsString,
    path: Option<&JsString>,
    limit: Option<usize>,
    signal: Option<&AbortSignal>,
) -> Result<AgentToolResult, ToolError> {
    let dir_path = resolve_to_cwd(path.cloned().unwrap_or_else(|| ".".into()), cwd)
        .map_err(|error| ToolError::new(error.message().clone()))?;
    let effective_limit = limit.unwrap_or(DEFAULT_LIMIT);
    if !operations
        .exists(&dir_path)
        .await
        .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?
    {
        return Err(ToolError::new(js_format!("Path not found: {}", dir_path)));
    }
    let is_directory = operations
        .is_directory(&dir_path)
        .await
        .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?;
    if !is_directory {
        return Err(ToolError::new(js_format!("Not a directory: {}", dir_path)));
    }
    let mut entries = operations.readdir(&dir_path).await.map_err(|error| {
        ToolError::new(JsString::from("Cannot read directory: ") + super::node_fs::operation_error_message(&error))
    })?;
    entries.sort_by(|left, right| locale_compare_utf16(left.to_lowercase().units(), right.to_lowercase().units()));
    check_aborted(signal)?;

    let mut results = Vec::new();
    let mut entry_limit_reached = false;
    for entry in &entries {
        if results.len() >= effective_limit {
            entry_limit_reached = true;
            break;
        }
        let full_path = node_join(&dir_path, entry);
        match operations.is_directory(&full_path).await {
            Ok(is_directory) => {
                results.push(if is_directory {
                    js_format!("{}/", entry)
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
    let raw_output = JsString::join(&results, "\n");
    let truncation = truncate_head(
        &raw_output,
        TruncationOptions {
            max_lines: MAX_SAFE_INTEGER as f64,
            max_bytes: DEFAULT_MAX_BYTES as f64,
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
        details.insert("entryLimitReached".into(), json!(effective_limit));
    }
    if truncation.truncated {
        notices.push(format!("{} limit reached", format_size(DEFAULT_MAX_BYTES as f64)));
        details.insert(
            "truncation".into(),
            pi_js_compat::js_json::to_value(&truncation).expect("truncation metadata must serialize"),
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

fn text_result(text: impl Into<JsString>, details: Option<Value>) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details,
        is_error: None,
        terminate: None,
    }
}

async fn execute_rtk_ls(
    cwd: &JsString,
    path: Option<&JsString>,
    limit: Option<usize>,
    signal: Option<&AbortSignal>,
) -> Result<Option<AgentToolResult>, ToolError> {
    match run_rtk_ls(cwd, &path.cloned().unwrap_or_else(|| ".".into()), signal).await {
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
        message: JsString,
        stdout: String,
        stderr: String,
    },
}

fn format_rtk_ls_error(message: &JsString, stdout: &str, stderr: &str) -> JsString {
    let stderr = stderr.trim();
    let stdout = stdout.trim();
    if !stderr.is_empty() {
        stderr.into()
    } else if !stdout.is_empty() {
        stdout.into()
    } else {
        message.clone()
    }
}

async fn run_rtk_ls(cwd: &JsString, path: &JsString, signal: Option<&AbortSignal>) -> Result<String, RtkError> {
    let mut command = tokio::process::Command::new("rtk");
    command
        .arg("ls")
        .arg(encode_node_utf8(path))
        .current_dir(encode_node_utf8(cwd))
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
                message: node_spawn_error_message("spawn", &pi_js_compat::js_json::JsString::from("rtk"), &error),
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
            message: "stdout maxBuffer length exceeded".into(),
            stdout: stdout[..RTK_MAX_BUFFER].to_owned(),
            stderr,
        });
    }
    match status {
        Ok(Some(status)) if status.success() => Ok(stdout),
        Ok(Some(status)) => Err(RtkError::Failed {
            message: js_format!("Command failed: rtk ls {} {}", path, status.to_string()),
            stdout,
            stderr,
        }),
        Ok(None) => Err(RtkError::Failed {
            message: "The operation was aborted".into(),
            stdout,
            stderr,
        }),
        Err(error) => Err(RtkError::Failed {
            message: error.to_string().into(),
            stdout,
            stderr,
        }),
    }
}

async fn wait_with_cancellation(
    child: &mut tokio::process::Child,
    signal: Option<&AbortSignal>,
) -> io::Result<Option<std::process::ExitStatus>> {
    let Some(signal) = signal else {
        return child.wait().await.map(Some);
    };
    tokio::select! {
        status = child.wait() => status.map(Some),
        _ = signal.wait() => {
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
    let raw_output = JsString::from(if limited_lines.is_empty() {
        "(empty directory)".to_owned()
    } else {
        limited_lines.join("\n")
    });
    let truncation = truncate_head(
        &raw_output,
        TruncationOptions {
            max_lines: MAX_SAFE_INTEGER as f64,
            max_bytes: DEFAULT_MAX_BYTES as f64,
        },
    );
    let mut details = Map::new();
    let mut notices = Vec::new();
    if entry_limit_reached {
        notices.push(format!(
            "{effective_limit} entries limit reached. Use limit={} for more",
            effective_limit * 2
        ));
        details.insert("entryLimitReached".into(), json!(effective_limit));
    }
    if truncation.truncated {
        notices.push(format!("{} limit reached", format_size(DEFAULT_MAX_BYTES as f64)));
        details.insert(
            "truncation".into(),
            pi_js_compat::js_json::to_value(&truncation).expect("truncation metadata must serialize"),
        );
    }
    let notice_text = if notices.is_empty() {
        String::new()
    } else {
        format!("\n\n[{}]", notices.join(". "))
    };
    text_result(
        truncation.content.clone() + notice_text,
        (!details.is_empty()).then_some(Value::Object(details)),
    )
}

fn check_aborted(signal: Option<&AbortSignal>) -> Result<(), ToolError> {
    if signal.is_some_and(AbortSignal::aborted) {
        return Err(ToolError::new("Operation aborted"));
    }
    Ok(())
}

// ============================================================================
// Renderers
// ============================================================================

fn format_ls_call(args: &JsValue, theme: &Theme, cwd: &JsString) -> Result<JsString, PathError> {
    let path_display = render_tool_path(str_value(args.get("path")).as_ref(), theme, cwd, Some("."))?;
    let mut text = js_format!("{} {}", theme.fg(ThemeColor::ToolTitle, theme.bold("ls")), path_display);
    if let Some(limit) = args.get("limit").and_then(Value::as_f64) {
        text.push_str(&theme.fg(
            ThemeColor::ToolOutput,
            &format!(" (limit {})", pi_js_compat::number::js_number_to_string(limit)),
        ));
    }
    Ok(text)
}

fn format_ls_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    show_images: bool,
) -> JsString {
    let output = get_text_output(Some(result), show_images).trim();
    let mut text = JsString::default();
    if !output.is_empty() {
        let lines = output.split('\n').collect::<Vec<_>>();
        let max_lines = if options.expanded { lines.len() } else { 20 };
        let display_lines = lines.iter().take(max_lines).collect::<Vec<_>>();
        let remaining = lines.len().saturating_sub(max_lines);
        text.push_str(js_format!(
            "\n{}",
            JsString::join(
                display_lines.iter().map(|line| theme.fg(ThemeColor::ToolOutput, *line)),
                "\n"
            )
        ));
        if remaining > 0 {
            text.push_str(js_format!(
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
        .and_then(|value| pi_js_compat::js_json::from_value::<TruncationResult>(value).ok());
    if entry_limit.is_some() || truncation.as_ref().is_some_and(|truncation| truncation.truncated) {
        let mut warnings = Vec::new();
        if let Some(entry_limit) = entry_limit {
            warnings.push(format!("{entry_limit} entries limit"));
        }
        if let Some(truncation) = truncation.filter(|truncation| truncation.truncated) {
            warnings.push(format!("{} limit", format_size(truncation.max_bytes)));
        }
        text.push_str(js_format!(
            "\n{}",
            theme.fg(ThemeColor::Warning, format!("[Truncated: {}]", warnings.join(", ")))
        ));
    }
    text
}

/// `renderCall` for the ls tool.
pub fn render_call(args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> Result<ComponentRef, HandlerError> {
    Ok(Rc::new(RefCell::new(Text::new(
        format_ls_call(args, theme, &context.cwd).map_err(|error| HandlerError::new(error.message().clone()))?,
        0,
        0,
    ))))
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
pub fn create_ls_tool_definition(cwd: impl Into<JsString>, options: LsToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(LsTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.prompt_snippet = Some(LS_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet.into());
    definition.approval_kind = Some(crate::core::extensions::types::ApprovalKind::ReadOnly);
    definition.render_call = Some(std::sync::Arc::new(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

/// `createLsTool`: the ls tool wrapped with the shared result cap.
pub fn create_ls_tool(cwd: impl Into<JsString>, options: LsToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(LsTool::with_options(cwd, options)))
}
