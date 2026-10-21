//! Ported from `packages/coding-agent/src/core/tools/grep.ts`.
//!
//! Runs the real `rg --json` binary, parses match events, adds context lines
//! from the filesystem, and applies the source limits. Renderers are pending
//! `theme.ts`.

use pi_js_compat::js_json::JsValue;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject as Map, JsString, JsValue as Value, encode_node_utf8};
use pi_tui::Text;
use pi_tui::tui::ComponentRef;
use serde_json::Value as SchemaValue;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio_util::sync::CancellationToken;

use super::path_utils::resolve_to_cwd;
use super::render_utils::{get_text_output, invalid_arg_text, shorten_path, str_value};
use super::tool_definition_wrapper::create_tool_definition_from_agent_tool;
use super::tool_definition_wrapper::wrap_agent_tool;
use super::truncate::{
    DEFAULT_MAX_BYTES, GREP_MAX_LINE_LENGTH, TruncationOptions, TruncationResult, format_size, truncate_head,
    truncate_line,
};
use crate::core::extensions::types::{
    ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_call, infallible_render_result,
};
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{Theme, ThemeColor};
use crate::utils::child_process::node_error_code;
use crate::utils::paths::relative_path;
use crate::utils::tools_manager::ToolsManager;

const DESCRIPTION_PREFIX: &str = "Search file contents for a pattern. Returns matching lines with file paths and line numbers. Respects .gitignore. Output is truncated to";
const DEFAULT_LIMIT: usize = 100;
const MAX_SAFE_INTEGER: usize = 9_007_199_254_740_991;

/// `GREP_TOOL_SYSTEM_PROMPT_CONTRIBUTION`.
pub const GREP_TOOL_SYSTEM_PROMPT_CONTRIBUTION: super::ToolSystemPromptContribution =
    super::ToolSystemPromptContribution {
        snippet: "Search file contents for patterns (respects .gitignore)",
        guidelines: &[],
    };

/// Filesystem access used by [`GrepTool`]. Override to delegate reads to a remote system.
pub trait GrepOperations: Send + Sync {
    fn is_directory<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<bool>>;
    fn read_file<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<String>>;
}

/// Local filesystem operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalGrepOperations;

impl GrepOperations for LocalGrepOperations {
    fn is_directory<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            tokio::fs::metadata(absolute_path)
                .await
                .map(|metadata| metadata.is_dir())
        })
    }

    fn read_file<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<String>> {
        Box::pin(async move { tokio::fs::read_to_string(absolute_path).await })
    }
}

/// Options for [`GrepTool`].
#[derive(Clone, Default)]
pub struct GrepToolOptions {
    /// Custom filesystem access. Default: local filesystem plus ripgrep.
    pub operations: Option<Arc<dyn GrepOperations>>,
}

/// Searches file contents with ripgrep.
pub struct GrepTool {
    cwd: String,
    parameters: SchemaValue,
    operations: Arc<dyn GrepOperations>,
}

impl GrepTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_options(cwd, GrepToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<String>, options: GrepToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: grep_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalGrepOperations)),
        }
    }
}

fn grep_schema() -> SchemaValue {
    serde_json::json!({
        "type": "object",
        "required": ["pattern"],
        "properties": {
            "pattern": {"type": "string", "description": "Search pattern (regex or literal string)"},
            "path": {"type": "string", "description": "Directory or file to search (default: current directory)"},
            "glob": {"type": "string", "description": "Filter files by glob pattern, e.g. '*.ts' or '**/*.spec.ts'"},
            "ignoreCase": {"type": "boolean", "description": "Case-insensitive search (default: false)"},
            "literal": {"type": "boolean", "description": "Treat pattern as literal string instead of regex (default: false)"},
            "context": {"type": "number", "description": "Number of lines to show before and after each match (default: 0)"},
            "limit": {"type": "number", "description": "Maximum number of matches to return (default: 100)"}
        }
    })
}

static DESCRIPTION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "{DESCRIPTION_PREFIX} {DEFAULT_LIMIT} matches or {}KB (whichever is hit first). Long lines are truncated to {GREP_MAX_LINE_LENGTH} chars.",
        DEFAULT_MAX_BYTES / 1024
    )
});

impl AgentTool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        DESCRIPTION.as_str()
    }

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> &str {
        "grep"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        signal: Option<CancellationToken>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let cwd = self.cwd.clone();
        let operations = Arc::clone(&self.operations);
        Box::pin(async move { execute_grep(&cwd, operations, params, signal).await })
    }
}

struct GrepInput {
    pattern: JsString,
    search_dir: Option<String>,
    glob: Option<String>,
    ignore_case: bool,
    literal: bool,
    context: usize,
    limit: Option<usize>,
}

fn parse_grep_input(params: &JsValue) -> Result<GrepInput, ToolError> {
    let pattern = params
        .get("pattern")
        .and_then(Value::as_string)
        .ok_or_else(|| ToolError::new("grep tool requires a string pattern"))?
        .clone();
    Ok(GrepInput {
        pattern,
        search_dir: params.get("path").and_then(Value::as_string).map(encode_node_utf8),
        glob: params.get("glob").and_then(Value::as_string).map(encode_node_utf8),
        ignore_case: params.get("ignoreCase").and_then(Value::as_bool).unwrap_or(false),
        literal: params.get("literal").and_then(Value::as_bool).unwrap_or(false),
        context: params
            .get("context")
            .and_then(Value::as_f64)
            .filter(|value| *value > 0.0)
            .map(|value| value.trunc() as usize)
            .unwrap_or(0),
        limit: params
            .get("limit")
            .and_then(Value::as_f64)
            .map(|value| value.trunc().max(0.0) as usize),
    })
}

async fn execute_grep(
    cwd: &str,
    operations: Arc<dyn GrepOperations>,
    params: JsValue,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let input = parse_grep_input(&params)?;
    check_aborted(signal.as_ref())?;
    let mut tools = ToolsManager::for_process(pi_ai::http::default_fetch());
    let Some(rg_path) = tools.ensure_tool("rg").await else {
        return Err(ToolError::new("ripgrep (rg) is not available on PATH"));
    };
    let search_path = resolve_to_cwd(input.search_dir.as_deref().unwrap_or("."), cwd)
        .map_err(|error| ToolError::new(error.to_string()))?;
    let is_directory = operations
        .is_directory(&search_path)
        .await
        .map_err(|_| ToolError::new(format!("Path not found: {search_path}")))?;
    let effective_limit = match input.limit {
        Some(limit) => limit.max(1),
        None => DEFAULT_LIMIT,
    };

    let matches = collect_matches(&rg_path, &input, &search_path, effective_limit, signal.as_ref()).await?;
    check_aborted(signal.as_ref())?;
    if matches.entries.is_empty() && !matches.killed_due_to_limit {
        return Ok(text_result("No matches found".to_owned(), None));
    }

    let mut lines_truncated = false;
    let mut output_lines = Vec::new();
    for entry in &matches.entries {
        if input.context == 0 && entry.line_text.is_some() {
            let relative = format_path(&search_path, is_directory, &entry.file_path);
            let raw_line = entry.line_text.as_ref().cloned().unwrap_or_default();
            let mut units = raw_line
                .units()
                .iter()
                .copied()
                .filter(|unit| *unit != 0x0d)
                .collect::<Vec<_>>();
            if units.last() == Some(&0x0a) {
                units.pop();
            }
            let sanitized = JsString::from_utf16(units);
            let truncated = truncate_line(&sanitized, None);
            lines_truncated |= truncated.was_truncated;
            output_lines.push(JsString::from(format!("{relative}:{}: ", entry.line_number)) + truncated.text);
        } else {
            let block = format_block(operations.as_ref(), &search_path, is_directory, entry, input.context).await;
            lines_truncated |= block.lines_truncated;
            output_lines.extend(block.lines);
        }
    }

    let raw_output = JsString::join(output_lines, "\n");
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
    if matches.killed_due_to_limit {
        notices.push(format!(
            "{effective_limit} matches limit reached. Use limit={} for more, or refine pattern",
            effective_limit * 2
        ));
        details.insert("matchLimitReached".into(), json!(effective_limit));
    }
    if truncation.truncated {
        notices.push(format!("{} limit reached", format_size(DEFAULT_MAX_BYTES)));
        details.insert(
            "truncation".into(),
            pi_js_compat::js_json::to_value(&truncation).expect("truncation metadata must serialize"),
        );
    }
    if lines_truncated {
        notices.push(format!(
            "Some lines truncated to {GREP_MAX_LINE_LENGTH} chars. Use read tool to see full lines"
        ));
        details.insert("linesTruncated".into(), Value::Bool(true));
    }
    if !notices.is_empty() {
        output.push_str(&format!("\n\n[{}]", notices.join(". ")));
    }
    Ok(text_result(
        output,
        (!details.is_empty()).then_some(Value::Object(details)),
    ))
}

struct MatchEntry {
    file_path: String,
    line_number: usize,
    line_text: Option<JsString>,
}

struct MatchCollection {
    entries: Vec<MatchEntry>,
    killed_due_to_limit: bool,
}

async fn collect_matches(
    rg_path: &str,
    input: &GrepInput,
    search_path: &str,
    effective_limit: usize,
    signal: Option<&CancellationToken>,
) -> Result<MatchCollection, ToolError> {
    let mut args = vec![
        "--json".to_owned(),
        "--max-columns".to_owned(),
        DEFAULT_MAX_BYTES.to_string(),
        "--line-number".to_owned(),
        "--color=never".to_owned(),
        "--hidden".to_owned(),
    ];
    if input.ignore_case {
        args.push("--ignore-case".to_owned());
    }
    if input.literal {
        args.push("--fixed-strings".to_owned());
    }
    if let Some(glob) = &input.glob {
        args.push("--glob".to_owned());
        args.push(glob.clone());
    }
    args.push("--".to_owned());
    args.push(encode_node_utf8(&input.pattern));
    args.push(search_path.to_owned());

    let mut command = tokio::process::Command::new(rg_path);
    command
        .args(&args)
        .env_clear()
        .envs(pi_js_compat::process_env::vars())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| {
        ToolError::new(format!(
            "Failed to run ripgrep: spawn {rg_path} {}",
            node_error_code(&error)
        ))
    })?;
    let stdout = child.stdout.take().expect("rg stdout");
    let mut stderr = child.stderr.take().expect("rg stderr");
    let stderr_task = tokio::spawn(async move {
        let mut buffer = String::new();
        let _ = stderr.read_to_string(&mut buffer).await;
        buffer
    });

    let mut entries = Vec::new();
    let mut match_count = 0;
    let mut killed_due_to_limit = false;
    let mut reader = BufReader::new(stdout).lines();
    loop {
        tokio::select! {
            line = reader.next_line() => {
                let Ok(Some(line)) = line else { break };
                if line.trim().is_empty() || match_count >= effective_limit {
                    continue;
                }
                let Ok(event) = serde_json::from_str::<Value>(&line) else { continue };
                if event.get("type").and_then(Value::as_string) != Some(&JsString::from("match")) {
                    continue;
                }
                match_count += 1;
                let file_path = event["data"]["path"]["text"].as_string();
                let line_number = event["data"]["line_number"].as_u64();
                let line_text = event["data"]["lines"]["text"].as_string().cloned();
                if let (Some(file_path), Some(line_number)) = (file_path, line_number) {
                    entries.push(MatchEntry {
                        file_path: encode_node_utf8(file_path),
                        line_number: line_number as usize,
                        line_text,
                    });
                }
                if match_count >= effective_limit {
                    killed_due_to_limit = true;
                    let _ = child.start_kill();
                    break;
                }
            }
            () = cancellation(signal) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(ToolError::new("Operation aborted"));
            }
        }
    }

    let status = child.wait().await.map_err(|error| ToolError::new(error.to_string()))?;
    let stderr = stderr_task.await.unwrap_or_default();
    if killed_due_to_limit {
        return Ok(MatchCollection {
            entries,
            killed_due_to_limit,
        });
    }
    let code = status.code();
    if code != Some(0) && code != Some(1) {
        let message = stderr.trim();
        let message = if message.is_empty() {
            format!(
                "ripgrep exited with code {}",
                code.map_or("null".to_owned(), |code| code.to_string())
            )
        } else {
            message.to_owned()
        };
        return Err(ToolError::new(message));
    }
    Ok(MatchCollection {
        entries,
        killed_due_to_limit,
    })
}

async fn cancellation(signal: Option<&CancellationToken>) {
    match signal {
        Some(signal) => signal.cancelled().await,
        None => std::future::pending().await,
    }
}

struct FormattedBlock {
    lines: Vec<JsString>,
    lines_truncated: bool,
}

async fn format_block(
    operations: &dyn GrepOperations,
    search_path: &str,
    is_directory: bool,
    entry: &MatchEntry,
    context: usize,
) -> FormattedBlock {
    let relative = format_path(search_path, is_directory, &entry.file_path);
    let file_lines = match operations.read_file(&entry.file_path).await {
        Ok(content) => content
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .map(str::to_owned)
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    if file_lines.is_empty() {
        return FormattedBlock {
            lines: vec![format!("{relative}:{}: (unable to read file)", entry.line_number).into()],
            lines_truncated: false,
        };
    }
    let start = if context > 0 {
        entry.line_number.saturating_sub(context).max(1)
    } else {
        entry.line_number
    };
    let end = if context > 0 {
        (entry.line_number + context).min(file_lines.len())
    } else {
        entry.line_number
    };
    let mut lines = Vec::new();
    let mut lines_truncated = false;
    for current in start..=end {
        let line_text = file_lines.get(current - 1).map(String::as_str).unwrap_or("");
        let sanitized = line_text.replace('\r', "");
        let truncated = truncate_line(&sanitized, None);
        lines_truncated |= truncated.was_truncated;
        if current == entry.line_number {
            lines.push(JsString::from(format!("{relative}:{current}: ")) + truncated.text);
        } else {
            lines.push(JsString::from(format!("{relative}-{current}- ")) + truncated.text);
        }
    }
    FormattedBlock { lines, lines_truncated }
}

fn format_path(search_path: &str, is_directory: bool, file_path: &str) -> String {
    if is_directory {
        let relative = relative_path(search_path, file_path);
        if !relative.is_empty() && !relative.starts_with("..") {
            return relative.replace('\\', "/");
        }
    }
    std::path::Path::new(file_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_path.to_owned())
}

fn text_result(text: impl Into<JsString>, details: Option<Value>) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details,
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

// ============================================================================
// Renderers
// ============================================================================

fn format_grep_call(args: &JsValue, theme: &Theme) -> String {
    let pattern = str_value(args.get("pattern"));
    let raw_path = str_value(args.get("path"));
    let path = raw_path.map(|path| {
        if path.is_empty() {
            shorten_path(".")
        } else {
            shorten_path(&path)
        }
    });
    let glob = str_value(args.get("glob"));
    let limit = args.get("limit").and_then(Value::as_f64);
    let invalid_arg = invalid_arg_text(theme);
    let mut text = format!(
        "{} {}{}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("grep")),
        match pattern {
            None => invalid_arg.clone(),
            Some(pattern) => theme.fg(ThemeColor::Accent, &format!("/{pattern}/")),
        },
        theme.fg(ThemeColor::ToolOutput, &format!(" in {}", path.unwrap_or(invalid_arg)))
    );
    if let Some(glob) = glob.filter(|glob| !glob.is_empty()) {
        text.push_str(&theme.fg(ThemeColor::ToolOutput, &format!(" ({glob})")));
    }
    if let Some(limit) = limit {
        text.push_str(&theme.fg(
            ThemeColor::ToolOutput,
            &format!(" limit {}", pi_js_compat::number::js_number_to_string(limit)),
        ));
    }
    text
}

fn format_grep_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    show_images: bool,
) -> String {
    let output = get_text_output(Some(result), show_images).trim().to_owned();
    let mut text = String::new();
    if !output.is_empty() {
        let lines = output.split('\n').collect::<Vec<_>>();
        let max_lines = if options.expanded { lines.len() } else { 15 };
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
    let details = result.details.as_ref();
    let match_limit = details
        .and_then(|details| details.get("matchLimitReached"))
        .and_then(Value::as_u64);
    let truncation = details
        .and_then(|details| details.get("truncation"))
        .and_then(|value| pi_js_compat::js_json::from_value::<TruncationResult>(value).ok());
    let lines_truncated = details
        .and_then(|details| details.get("linesTruncated"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if match_limit.is_some() || truncation.as_ref().is_some_and(|value| value.truncated) || lines_truncated {
        let mut warnings = Vec::new();
        if let Some(match_limit) = match_limit {
            warnings.push(format!("{match_limit} matches limit"));
        }
        if let Some(truncation) = truncation.filter(|value| value.truncated) {
            warnings.push(format!("{} limit", format_size(truncation.max_bytes)));
        }
        if lines_truncated {
            warnings.push("some lines truncated".to_owned());
        }
        text.push_str(&format!(
            "\n{}",
            theme.fg(ThemeColor::Warning, &format!("[Truncated: {}]", warnings.join(", ")))
        ));
    }
    text
}

/// `renderCall` for the grep tool.
pub fn render_call(args: &JsValue, theme: &Theme, _context: &ToolRenderContext) -> ComponentRef {
    Rc::new(RefCell::new(Text::new(format_grep_call(args, theme), 0, 0)))
}

/// `renderResult` for the grep tool.
pub fn render_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    let output = if context.is_error {
        format_grep_result(result, options, theme, context.show_images)
    } else {
        String::new()
    };
    Rc::new(RefCell::new(Text::new(output, 0, 0)))
}

/// `createGrepToolDefinition`: the grep tool plus its renderers.
pub fn create_grep_tool_definition(cwd: &str, options: GrepToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(GrepTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.prompt_snippet = Some(GREP_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet.to_owned());
    definition.approval_kind = Some(crate::core::extensions::types::ApprovalKind::ReadOnly);
    definition.approval_required = Some(false);
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

/// `createGrepTool`: the grep tool wrapped with the shared result cap.
pub fn create_grep_tool(cwd: &str, options: GrepToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(GrepTool::with_options(cwd, options)))
}
