//! Ported from `packages/coding-agent/src/core/tools/grep.ts`.
//!
//! Runs the real `rg --json` binary, parses match events, adds context lines
//! from the filesystem, and applies the source limits. Renderers are pending
//! `theme.ts`.

use crate::utils::paths::node_basename;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::node_error::NativeNodeError;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject as Map, JsString, JsValue as Value, encode_node_utf8};
use pi_tui::Text;
use pi_tui::tui::ComponentRef;
use serde_json::Value as SchemaValue;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

use super::path_utils::resolve_to_cwd;
use super::render_utils::{get_text_output, invalid_arg_text, shorten_path, str_value};
use super::tool_definition_wrapper::create_tool_definition_from_agent_tool;
use super::tool_definition_wrapper::wrap_agent_tool;
use super::truncate::{
    DEFAULT_MAX_BYTES, GREP_MAX_LINE_LENGTH, TruncationOptions, TruncationResult, format_size, truncate_head,
    truncate_line,
};
use crate::core::extensions::types::{
    HandlerError, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_result,
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
    fn is_directory<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<bool>>;
    fn read_file<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<JsString>>;
}

/// Local filesystem operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalGrepOperations;

impl GrepOperations for LocalGrepOperations {
    fn is_directory<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            super::node_fs::validate_node_path(absolute_path)?;
            tokio::fs::metadata(encode_node_utf8(absolute_path))
                .await
                .map(|metadata| metadata.is_dir())
        })
    }

    fn read_file<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<JsString>> {
        Box::pin(async move {
            super::node_fs::validate_node_path(absolute_path)?;
            tokio::fs::read(encode_node_utf8(absolute_path))
                .await
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned().into())
        })
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
    cwd: JsString,
    parameters: SchemaValue,
    operations: Arc<dyn GrepOperations>,
}

impl GrepTool {
    pub fn new(cwd: impl Into<JsString>) -> Self {
        Self::with_options(cwd, GrepToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<JsString>, options: GrepToolOptions) -> Self {
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

    fn description(&self) -> pi_js_compat::js_json::JsString {
        DESCRIPTION.as_str().into()
    }

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "grep".into()
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
        Box::pin(async move { execute_grep(&cwd, operations, params, signal).await })
    }
}

struct GrepInput {
    pattern: JsString,
    search_dir: Option<JsString>,
    glob: Option<JsString>,
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
        search_dir: params.get("path").and_then(Value::as_string).cloned(),
        glob: params.get("glob").and_then(Value::as_string).cloned(),
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
    cwd: &JsString,
    operations: Arc<dyn GrepOperations>,
    params: JsValue,
    signal: Option<AbortSignal>,
) -> Result<AgentToolResult, ToolError> {
    let input = parse_grep_input(&params)?;
    check_aborted(signal.as_ref())?;
    let mut tools = ToolsManager::for_process(pi_ai::http::default_fetch())
        .map_err(|error| ToolError::new(error.message().clone()))?;
    let Some(rg_path) = tools.ensure_tool("rg").await else {
        return Err(ToolError::new("ripgrep (rg) is not available on PATH"));
    };
    let search_path = resolve_to_cwd(input.search_dir.clone().unwrap_or_else(|| ".".into()), cwd)
        .map_err(|error| ToolError::new(error.message().clone()))?;
    let is_directory = operations
        .is_directory(&search_path)
        .await
        .map_err(|_| ToolError::new(js_format!("Path not found: {}", search_path)))?;
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
            output_lines.push(js_format!("{}:{}: ", relative, entry.line_number.to_string()) + truncated.text);
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
            max_lines: MAX_SAFE_INTEGER as f64,
            max_bytes: DEFAULT_MAX_BYTES as f64,
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
        notices.push(format!("{} limit reached", format_size(DEFAULT_MAX_BYTES as f64)));
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
    file_path: JsString,
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
    search_path: &JsString,
    effective_limit: usize,
    signal: Option<&AbortSignal>,
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
        args.push(encode_node_utf8(glob));
    }
    args.push("--".to_owned());
    args.push(encode_node_utf8(&input.pattern));
    args.push(encode_node_utf8(search_path));

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
                        file_path: file_path.clone(),
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

async fn cancellation(signal: Option<&AbortSignal>) {
    match signal {
        Some(signal) => {
            signal.wait().await;
        }
        None => std::future::pending().await,
    }
}

struct FormattedBlock {
    lines: Vec<JsString>,
    lines_truncated: bool,
}

async fn format_block(
    operations: &dyn GrepOperations,
    search_path: &JsString,
    is_directory: bool,
    entry: &MatchEntry,
    context: usize,
) -> FormattedBlock {
    let relative = format_path(search_path, is_directory, &entry.file_path);
    let file_lines = match operations.read_file(&entry.file_path).await {
        Ok(content) => content
            .replace("\r\n", "\n")
            .replace("\r", "\n")
            .split('\n')
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    if file_lines.is_empty() {
        return FormattedBlock {
            lines: vec![js_format!(
                "{}:{}: (unable to read file)",
                relative,
                entry.line_number.to_string()
            )],
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
        let line_text = file_lines.get(current - 1).cloned().unwrap_or_default();
        let sanitized = line_text.replace("\r", "");
        let truncated = truncate_line(&sanitized, None);
        lines_truncated |= truncated.was_truncated;
        if current == entry.line_number {
            lines.push(js_format!("{}:{}: ", relative, current.to_string()) + truncated.text);
        } else {
            lines.push(js_format!("{}-{}- ", relative, current.to_string()) + truncated.text);
        }
    }
    FormattedBlock { lines, lines_truncated }
}

fn format_path(search_path: &JsString, is_directory: bool, file_path: &JsString) -> JsString {
    if is_directory {
        let relative = relative_path(search_path, file_path).expect("absolute grep search root must resolve");
        if !relative.is_empty() && !relative.starts_with("..") {
            return relative.replace("\\", "/");
        }
    }
    node_basename(file_path)
}

fn text_result(text: impl Into<JsString>, details: Option<Value>) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details,
        is_error: None,
        terminate: None,
    }
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

fn format_grep_call(args: &JsValue, theme: &Theme) -> Result<JsString, NativeNodeError> {
    let pattern = str_value(args.get("pattern"));
    let raw_path = str_value(args.get("path"));
    let path = raw_path
        .map(|path| {
            if path.is_empty() {
                shorten_path(&JsString::from("."))
            } else {
                shorten_path(&path)
            }
        })
        .transpose()?;
    let glob = str_value(args.get("glob"));
    let limit = args.get("limit").and_then(Value::as_f64);
    let invalid_arg = invalid_arg_text(theme);
    let mut text = js_format!(
        "{} {}{}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("grep")),
        match pattern {
            None => invalid_arg.clone(),
            Some(pattern) => theme.fg(ThemeColor::Accent, js_format!("/{}/", pattern)),
        },
        theme.fg(
            ThemeColor::ToolOutput,
            js_format!(" in {}", path.unwrap_or(invalid_arg))
        )
    );
    if let Some(glob) = glob.filter(|glob| !glob.is_empty()) {
        text.push_str(&theme.fg(ThemeColor::ToolOutput, js_format!(" ({})", glob)));
    }
    if let Some(limit) = limit {
        text.push_str(&theme.fg(
            ThemeColor::ToolOutput,
            &format!(" limit {}", pi_js_compat::number::js_number_to_string(limit)),
        ));
    }
    Ok(text)
}

fn format_grep_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    show_images: bool,
) -> JsString {
    let output = get_text_output(Some(result), show_images).trim().to_owned();
    let mut text = JsString::default();
    if !output.is_empty() {
        let lines = output.split('\n').collect::<Vec<_>>();
        let max_lines = if options.expanded { lines.len() } else { 15 };
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
        text.push_str(js_format!(
            "\n{}",
            theme.fg(ThemeColor::Warning, &format!("[Truncated: {}]", warnings.join(", ")))
        ));
    }
    text
}

/// `renderCall` for the grep tool.
pub fn render_call(args: &JsValue, theme: &Theme, _context: &ToolRenderContext) -> Result<ComponentRef, HandlerError> {
    Ok(Rc::new(RefCell::new(Text::new(
        format_grep_call(args, theme).map_err(HandlerError::from)?,
        0,
        0,
    ))))
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
        JsString::default()
    };
    Rc::new(RefCell::new(Text::new(output, 0, 0)))
}

/// `createGrepToolDefinition`: the grep tool plus its renderers.
pub fn create_grep_tool_definition(cwd: impl Into<JsString>, options: GrepToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(GrepTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.prompt_snippet = Some(GREP_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet.into());
    definition.approval_kind = Some(crate::core::extensions::types::ApprovalKind::ReadOnly);
    definition.approval_required = Some(false);
    definition.render_call = Some(std::sync::Arc::new(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

#[cfg(test)]
mod lossless_context_tests {
    use super::*;
    use std::sync::Mutex;

    struct ContextOperations {
        paths: Mutex<Vec<JsString>>,
        content: JsString,
    }
    impl GrepOperations for ContextOperations {
        fn is_directory<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
            Box::pin(async { Ok(true) })
        }
        fn read_file<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<JsString>> {
            Box::pin(async move {
                self.paths.lock().unwrap().push(path.clone());
                Ok(self.content.clone())
            })
        }
    }

    #[tokio::test]
    async fn context_read_and_prefixes_keep_distinct_raw_units() {
        let root = JsString::from("/root-") + JsString::from_utf16(vec![0xd800]);
        let file = JsString::from("file-") + JsString::from_utf16(vec![0xdfff]);
        let full_path = root.clone() + "/" + &file;
        let content = JsString::from_utf16(vec![0xd801])
            + "\r\n"
            + JsString::from_utf16(vec![0xd802])
            + "\r"
            + JsString::from_utf16(vec![0xd803]);
        let operations = ContextOperations {
            paths: Mutex::new(Vec::new()),
            content,
        };
        let entry = MatchEntry {
            file_path: full_path.clone(),
            line_number: 2,
            line_text: None,
        };
        let block = format_block(&operations, &root, true, &entry, 1).await;
        assert_eq!(*operations.paths.lock().unwrap(), vec![full_path]);
        assert_eq!(
            block.lines,
            vec![
                file.clone() + "-1- " + JsString::from_utf16(vec![0xd801]),
                file.clone() + ":2: " + JsString::from_utf16(vec![0xd802]),
                file + "-3- " + JsString::from_utf16(vec![0xd803])
            ]
        );
        assert!(!block.lines_truncated);
    }
}

/// `createGrepTool`: the grep tool wrapped with the shared result cap.
pub fn create_grep_tool(cwd: impl Into<JsString>, options: GrepToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(GrepTool::with_options(cwd, options)))
}
