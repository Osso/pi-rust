//! Ported from `packages/coding-agent/src/core/tools/find.ts`.
//!
//! Runs the real `fd` binary with the source argv, relativizes results, and
//! applies the source limits. Renderers are pending `theme.ts`.

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

use super::path_utils::{path_exists, resolve_to_cwd};
use super::render_utils::{get_text_output, invalid_arg_text, shorten_path, str_value};
use super::tool_definition_wrapper::create_tool_definition_from_agent_tool;
use super::tool_definition_wrapper::wrap_agent_tool;
use super::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, TruncationResult, format_size, truncate_head};
use crate::core::extensions::types::{
    ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_call, infallible_render_result,
};
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{Theme, ThemeColor};
use crate::utils::child_process::node_error_code;
use crate::utils::tools_manager::ToolsManager;

const DESCRIPTION_PREFIX: &str = "Search for files by glob pattern. Returns matching file paths relative to the search directory. Respects .gitignore. Output is truncated to";
const DEFAULT_LIMIT: usize = 1000;
const MAX_SAFE_INTEGER: usize = 9_007_199_254_740_991;

/// `FIND_TOOL_SYSTEM_PROMPT_CONTRIBUTION`.
pub const FIND_TOOL_SYSTEM_PROMPT_CONTRIBUTION: super::ToolSystemPromptContribution =
    super::ToolSystemPromptContribution {
        snippet: "Find files by glob pattern (respects .gitignore)",
        guidelines: &[],
    };

/// Filesystem access used by [`FindTool`]. Override to delegate searches to a remote system.
pub trait FindOperations: Send + Sync {
    fn exists<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<bool>>;

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
    ) -> BoxFuture<'a, io::Result<Vec<String>>> {
        let _ = (pattern, cwd, ignore, limit);
        Box::pin(async { Ok(Vec::new()) })
    }
}

/// Local filesystem operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalFindOperations;

impl FindOperations for LocalFindOperations {
    fn exists<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move { Ok(path_exists(absolute_path).await) })
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
    parameters: SchemaValue,
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

fn find_schema() -> SchemaValue {
    serde_json::json!({
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

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> &str {
        "find"
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
        let operations = self.operations.clone();
        Box::pin(async move {
            let result = crate::utils::abort::race_with_abort_signal(
                execute_find(&cwd, operations, params, signal.clone()),
                signal.as_ref(),
            )
            .await;
            check_aborted(signal.as_ref())?;
            result.map_err(|_| ToolError::new("Operation aborted"))?
        })
    }
}

struct FindInput {
    pattern: JsString,
    search_dir: Option<String>,
    limit: Option<usize>,
}

fn parse_find_input(params: &JsValue) -> Result<FindInput, ToolError> {
    let pattern = params
        .get("pattern")
        .and_then(Value::as_string)
        .ok_or_else(|| ToolError::new("find tool requires a string pattern"))?
        .clone();
    Ok(FindInput {
        pattern,
        search_dir: params.get("path").and_then(Value::as_string).map(encode_node_utf8),
        limit: params
            .get("limit")
            .and_then(Value::as_f64)
            .map(|value| value.trunc().max(0.0) as usize),
    })
}

async fn execute_find(
    cwd: &str,
    operations: Option<Arc<dyn FindOperations>>,
    params: JsValue,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let input = parse_find_input(&params)?;
    check_aborted(signal.as_ref())?;
    let search_path = resolve_to_cwd(input.search_dir.as_deref().unwrap_or("."), cwd)
        .map_err(|error| ToolError::new(error.to_string()))?;
    let effective_limit = input.limit.unwrap_or(DEFAULT_LIMIT);

    if let Some(custom) = operations.as_deref().filter(|operations| operations.supports_glob()) {
        if !custom
            .exists(&search_path)
            .await
            .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?
        {
            return Err(ToolError::new(format!("Path not found: {search_path}")));
        }
        check_aborted(signal.as_ref())?;
        let results = custom
            .glob(
                &encode_node_utf8(&input.pattern),
                &search_path,
                &["**/node_modules/**".to_owned(), "**/.git/**".to_owned()],
                effective_limit,
            )
            .await
            .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?;
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
            effective_pattern = JsString::from("**/") + &input.pattern;
        }
        if cfg!(windows) {
            effective_pattern = JsString::join(effective_pattern.split('/'), "[/\\\\]");
        }
    }
    args.push("--".to_owned());
    args.push(encode_node_utf8(&effective_pattern));
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
) -> (JsString, Option<Value>) {
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
        details.insert("resultLimitReached".into(), json!(effective_limit));
    }
    if truncation.truncated {
        notices.push(format!("{} limit reached", format_size(DEFAULT_MAX_BYTES)));
        details.insert(
            "truncation".into(),
            pi_js_compat::js_json::to_value(&truncation).expect("truncation metadata must serialize"),
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
        .env_clear()
        .envs(pi_js_compat::process_env::vars())
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
    let status = child
        .wait()
        .await
        .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?;
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

fn format_find_call(args: &JsValue, theme: &Theme) -> String {
    let pattern = str_value(args.get("pattern"));
    let raw_path = str_value(args.get("path"));
    let path = raw_path.map(|path| {
        if path.is_empty() {
            shorten_path(".")
        } else {
            shorten_path(&path)
        }
    });
    let limit = args.get("limit").and_then(Value::as_f64);
    let invalid_arg = invalid_arg_text(theme);
    let mut text = format!(
        "{} {}{}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("find")),
        match pattern {
            None => invalid_arg.clone(),
            Some(pattern) => theme.fg(ThemeColor::Accent, &pattern),
        },
        theme.fg(ThemeColor::ToolOutput, &format!(" in {}", path.unwrap_or(invalid_arg)))
    );
    if let Some(limit) = limit {
        text.push_str(&theme.fg(
            ThemeColor::ToolOutput,
            &format!(" (limit {})", pi_js_compat::number::js_number_to_string(limit)),
        ));
    }
    text
}

fn format_find_result(
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
    let details = result.details.as_ref();
    let result_limit = details
        .and_then(|details| details.get("resultLimitReached"))
        .and_then(Value::as_u64);
    let truncation = details
        .and_then(|details| details.get("truncation"))
        .and_then(|value| pi_js_compat::js_json::from_value::<TruncationResult>(value).ok());
    if result_limit.is_some() || truncation.as_ref().is_some_and(|value| value.truncated) {
        let mut warnings = Vec::new();
        if let Some(result_limit) = result_limit {
            warnings.push(format!("{result_limit} results limit"));
        }
        if let Some(truncation) = truncation.filter(|value| value.truncated) {
            warnings.push(format!("{} limit", format_size(truncation.max_bytes)));
        }
        text.push_str(&format!(
            "\n{}",
            theme.fg(ThemeColor::Warning, &format!("[Truncated: {}]", warnings.join(", ")))
        ));
    }
    text
}

/// `renderCall` for the find tool.
pub fn render_call(args: &JsValue, theme: &Theme, _context: &ToolRenderContext) -> ComponentRef {
    Rc::new(RefCell::new(Text::new(format_find_call(args, theme), 0, 0)))
}

/// `renderResult` for the find tool.
pub fn render_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    Rc::new(RefCell::new(Text::new(
        format_find_result(result, options, theme, context.show_images),
        0,
        0,
    )))
}

/// `createFindToolDefinition`: the find tool plus its renderers.
pub fn create_find_tool_definition(cwd: &str, options: FindToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(FindTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.prompt_snippet = Some(FIND_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet.to_owned());
    definition.approval_kind = Some(crate::core::extensions::types::ApprovalKind::ReadOnly);
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

/// `createFindTool`: the find tool wrapped with the shared result cap.
pub fn create_find_tool(cwd: &str, options: FindToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(FindTool::with_options(cwd, options)))
}
