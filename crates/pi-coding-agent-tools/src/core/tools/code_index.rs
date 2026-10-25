//! Ported from `packages/coding-agent/src/core/tools/code-index.ts`.
//!
//! Thin wrappers over the external `code-index` CLI. Renderers are pending
//! `theme.ts`.

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
use tokio::io::AsyncReadExt;

use super::render_utils::{get_text_output, invalid_arg_text, shorten_path, str_value};
use super::tool_definition_wrapper::create_tool_definition_from_agent_tool;
use super::tool_definition_wrapper::wrap_agent_tool;
use super::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, TruncationResult, format_size, truncate_head};
use crate::core::extensions::types::{
    HandlerError, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, ToolRenderer, infallible_render_result,
};
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{Theme, ThemeColor};
use crate::utils::child_process::node_error_code;

const MAX_SAFE_INTEGER: usize = 9_007_199_254_740_991;
const SYMBOL_KINDS: [&str; 9] = [
    "function",
    "method",
    "class",
    "trait",
    "interface",
    "struct",
    "enum",
    "property",
    "event",
];
const REFERENCE_KINDS: [&str; 5] = ["call", "inherit", "implement", "import", "trait_impl"];

/// Result of running the external CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeIndexCommandResult {
    pub stdout: JsString,
    pub stderr: JsString,
    pub exit_code: Option<i32>,
}

/// External CLI runner used by the code-index tools.
pub trait CodeIndexOperations: Send + Sync {
    fn run<'a>(
        &'a self,
        args: &'a [JsString],
        cwd: &'a JsString,
        signal: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, io::Result<CodeIndexCommandResult>>;
}

/// Runs the locally installed `code-index` binary.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalCodeIndexOperations;

impl CodeIndexOperations for LocalCodeIndexOperations {
    fn run<'a>(
        &'a self,
        args: &'a [JsString],
        cwd: &'a JsString,
        signal: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, io::Result<CodeIndexCommandResult>> {
        Box::pin(async move { run_code_index_command("code-index", args, cwd, signal).await })
    }
}

/// Options for the code-index tools.
#[derive(Clone, Default)]
pub struct CodeIndexToolOptions {
    /// Custom command runner. Default: local `code-index` CLI.
    pub operations: Option<Arc<dyn CodeIndexOperations>>,
}

async fn run_code_index_command(
    code_index_path: &str,
    args: &[JsString],
    cwd: &JsString,
    signal: Option<&AbortSignal>,
) -> io::Result<CodeIndexCommandResult> {
    if signal.is_some_and(AbortSignal::aborted) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "aborted"));
    }
    let mut command = tokio::process::Command::new(code_index_path);
    command
        .args(args.iter().map(encode_node_utf8))
        .current_dir(encode_node_utf8(cwd))
        .env_clear()
        .envs(pi_js_compat::process_env::vars())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let mut stdout = child.stdout.take().expect("code-index stdout");
    let mut stderr = child.stderr.take().expect("code-index stderr");
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
    let status = wait_with_cancellation(&mut child, signal).await?;
    let stdout = String::from_utf8_lossy(&stdout_task.await.unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_task.await.unwrap_or_default()).into_owned();
    Ok(CodeIndexCommandResult {
        stdout: stdout.into(),
        stderr: stderr.into(),
        exit_code: status.and_then(|status| status.code()),
    })
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
            let _ = child.start_kill();
            let _ = child.wait().await;
            Err(io::Error::new(io::ErrorKind::Interrupted, "aborted"))
        }
    }
}

fn execute_code_index(
    operations: Arc<dyn CodeIndexOperations>,
    cwd: JsString,
    args: Vec<JsString>,
    signal: Option<AbortSignal>,
) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
    Box::pin(async move {
        let result = operations.run(&args, &cwd, signal.as_ref()).await.map_err(|error| {
            if signal.as_ref().is_some_and(AbortSignal::aborted) {
                ToolError::new("Operation aborted")
            } else {
                ToolError::new(format!(
                    "Failed to run code-index: spawn code-index {}",
                    node_error_code(&error)
                ))
            }
        })?;
        if signal.as_ref().is_some_and(AbortSignal::aborted) {
            return Err(ToolError::new("Operation aborted"));
        }
        if result.exit_code != Some(0) {
            let message = if !result.stderr.trim().is_empty() {
                result.stderr.trim().to_owned()
            } else if !result.stdout.trim().is_empty() {
                result.stdout.trim().to_owned()
            } else {
                JsString::from(format!(
                    "code-index exited with code {}",
                    result.exit_code.map_or("null".to_owned(), |code| code.to_string())
                ))
            };
            return Err(ToolError::new(message));
        }

        let raw_output = result.stdout.trim_end();
        let output = if raw_output.is_empty() {
            JsString::from("No results found")
        } else {
            raw_output.to_owned()
        };
        let truncation = truncate_head(
            &output,
            TruncationOptions {
                max_lines: MAX_SAFE_INTEGER as f64,
                max_bytes: DEFAULT_MAX_BYTES as f64,
            },
        );
        let mut text = truncation.content.clone();
        let mut details = Map::new();
        if truncation.truncated {
            text.push_str(format!("\n\n[{} limit reached]", format_size(DEFAULT_MAX_BYTES as f64)));
            details.insert(
                "truncation".into(),
                pi_js_compat::js_json::to_value(&truncation).expect("truncation metadata must serialize"),
            );
        }
        Ok(AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(text))],
            details: (!details.is_empty()).then_some(Value::Object(details)),
            is_error: None,
            terminate: None,
        })
    })
}

/// `outline` tool.
pub struct OutlineTool {
    cwd: JsString,
    parameters: SchemaValue,
    operations: Arc<dyn CodeIndexOperations>,
}

impl OutlineTool {
    pub fn new(cwd: impl Into<JsString>) -> Self {
        Self::with_options(cwd, CodeIndexToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<JsString>, options: CodeIndexToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: outline_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalCodeIndexOperations)),
        }
    }
}

fn outline_schema() -> SchemaValue {
    serde_json::json!({
        "type": "object",
        "required": ["path"],
        "properties": {
            "path": {"type": "string", "description": "File or directory to outline"},
            "digest": {"type": "boolean", "description": "Show compact directory digest (default: false)"},
            "glob": {"type": "string", "description": "Filter files by glob pattern, e.g. '**/*.ts'"},
            "show": {"type": "string", "description": "Show one symbol body or section from the outline"}
        }
    })
}

impl AgentTool for OutlineTool {
    fn name(&self) -> &str {
        "outline"
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "Show a structural outline for a file or directory using the external code-index CLI. Output is truncated to 10KB.".into()
    }

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "outline".into()
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        signal: Option<AbortSignal>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let mut args = vec!["outline".into()];
        args.push(string_argument(&params, "path").unwrap_or_default());
        if bool_argument(&params, "digest") {
            args.push("--digest".into());
        }
        if let Some(glob) = string_argument(&params, "glob") {
            args.push("--glob".into());
            args.push(glob);
        }
        if let Some(show) = string_argument(&params, "show") {
            args.push("--show".into());
            args.push(show);
        }
        execute_code_index(Arc::clone(&self.operations), self.cwd.clone(), args, signal)
    }
}

/// `symbol` tool.
pub struct SymbolTool {
    cwd: JsString,
    parameters: SchemaValue,
    operations: Arc<dyn CodeIndexOperations>,
}

impl SymbolTool {
    pub fn new(cwd: impl Into<JsString>) -> Self {
        Self::with_options(cwd, CodeIndexToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<JsString>, options: CodeIndexToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: symbol_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalCodeIndexOperations)),
        }
    }
}

fn literal_union(kinds: &[&str]) -> Value {
    json!({
        "anyOf": kinds
            .iter()
            .map(|kind| json!({"type": "string", "const": kind}))
            .collect::<Vec<_>>()
    })
}

fn symbol_schema() -> SchemaValue {
    serde_json::json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": {"type": "string", "description": "Symbol name to find"},
            "kind": literal_union(&SYMBOL_KINDS),
            "file": {"type": "string", "description": "Filter by file path substring"}
        }
    })
}

impl AgentTool for SymbolTool {
    fn name(&self) -> &str {
        "symbol"
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "Find symbol definitions by name using the external code-index CLI. Output is truncated to 10KB.".into()
    }

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "symbol".into()
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        signal: Option<AbortSignal>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let mut args = vec!["symbol".into()];
        args.push(string_argument(&params, "name").unwrap_or_default());
        if let Some(kind) = string_argument(&params, "kind") {
            args.push("--kind".into());
            args.push(kind);
        }
        if let Some(file) = string_argument(&params, "file") {
            args.push("--file".into());
            args.push(file);
        }
        execute_code_index(Arc::clone(&self.operations), self.cwd.clone(), args, signal)
    }
}

/// `references` tool.
pub struct ReferencesTool {
    cwd: JsString,
    parameters: SchemaValue,
    operations: Arc<dyn CodeIndexOperations>,
}

impl ReferencesTool {
    pub fn new(cwd: impl Into<JsString>) -> Self {
        Self::with_options(cwd, CodeIndexToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<JsString>, options: CodeIndexToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: references_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalCodeIndexOperations)),
        }
    }
}

fn references_schema() -> SchemaValue {
    serde_json::json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": {"type": "string", "description": "Symbol name to find references for"},
            "kind": literal_union(&REFERENCE_KINDS)
        }
    })
}

impl AgentTool for ReferencesTool {
    fn name(&self) -> &str {
        "references"
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "Find structural references to a symbol using the external code-index CLI. Output is truncated to 10KB.".into()
    }

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "references".into()
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        signal: Option<AbortSignal>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let mut args = vec!["references".into()];
        args.push(string_argument(&params, "name").unwrap_or_default());
        if let Some(kind) = string_argument(&params, "kind") {
            args.push("--kind".into());
            args.push(kind);
        }
        execute_code_index(Arc::clone(&self.operations), self.cwd.clone(), args, signal)
    }
}

fn string_argument(params: &JsValue, name: &str) -> Option<JsString> {
    params.get(name).and_then(Value::as_string).cloned()
}

fn bool_argument(params: &JsValue, name: &str) -> bool {
    params.get(name).and_then(Value::as_bool).unwrap_or(false)
}

// ============================================================================
// Renderers
// ============================================================================

fn format_code_index_call(
    tool_name: &str,
    target: Option<JsString>,
    theme: &Theme,
    suffix: Option<JsString>,
) -> Result<JsString, NativeNodeError> {
    let invalid_arg = invalid_arg_text(theme);
    let mut text = js_format!(
        "{} {}",
        theme.fg(ThemeColor::ToolTitle, theme.bold(tool_name)),
        match target {
            None => invalid_arg,
            Some(target) => theme.fg(ThemeColor::Accent, &shorten_path(&target)?),
        }
    );
    if let Some(suffix) = suffix.filter(|suffix| !suffix.is_empty()) {
        text.push_str(theme.fg(ThemeColor::ToolOutput, js_format!(" {}", suffix)));
    }
    Ok(text)
}

fn format_code_index_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    show_images: bool,
) -> JsString {
    let output = get_text_output(Some(result), show_images).trim().to_owned();
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
                theme.fg(ThemeColor::Muted, format!("\n... ({remaining} more lines,")),
                key_hint("app.tools.expand", "to expand"),
                theme.fg(ThemeColor::Muted, ")")
            ));
        }
    }
    if let Some(truncation) = result
        .details
        .as_ref()
        .and_then(|details| details.get("truncation"))
        .and_then(|value| pi_js_compat::js_json::from_value::<TruncationResult>(value).ok())
        .filter(|truncation| truncation.truncated)
    {
        text.push_str(js_format!(
            "\n{}",
            theme.fg(
                ThemeColor::Warning,
                format!("[Truncated: {} limit]", format_size(truncation.max_bytes))
            )
        ));
    }
    text
}

/// `renderCall` for the outline tool.
pub fn outline_render_call(
    args: &JsValue,
    theme: &Theme,
    _context: &ToolRenderContext,
) -> Result<ComponentRef, HandlerError> {
    let suffix = {
        let mut parts = Vec::new();
        if args.get("digest").and_then(Value::as_bool).unwrap_or(false) {
            parts.push(JsString::from("digest"));
        }
        if let Some(glob) = str_value(args.get("glob")).filter(|glob| !glob.is_empty()) {
            parts.push(glob);
        }
        if let Some(show) = str_value(args.get("show")).filter(|show| !show.is_empty()) {
            parts.push(js_format!("show {}", show));
        }
        (!parts.is_empty()).then(|| js_format!("({})", JsString::join(&parts, ", ")))
    };
    Ok(Rc::new(RefCell::new(Text::new(
        format_code_index_call("outline", str_value(args.get("path")), theme, suffix).map_err(HandlerError::from)?,
        0,
        0,
    ))))
}

/// `renderCall` for the symbol tool.
pub fn symbol_render_call(
    args: &JsValue,
    theme: &Theme,
    _context: &ToolRenderContext,
) -> Result<ComponentRef, HandlerError> {
    let suffix = {
        let mut parts = Vec::new();
        if let Some(kind) = str_value(args.get("kind")).filter(|kind| !kind.is_empty()) {
            parts.push(kind);
        }
        if let Some(file) = str_value(args.get("file")).filter(|file| !file.is_empty()) {
            parts.push(js_format!("in {}", shorten_path(&file).map_err(HandlerError::from)?));
        }
        (!parts.is_empty()).then(|| js_format!("({})", JsString::join(&parts, ", ")))
    };
    Ok(Rc::new(RefCell::new(Text::new(
        format_code_index_call("symbol", str_value(args.get("name")), theme, suffix).map_err(HandlerError::from)?,
        0,
        0,
    ))))
}

/// `renderCall` for the references tool.
pub fn references_render_call(
    args: &JsValue,
    theme: &Theme,
    _context: &ToolRenderContext,
) -> Result<ComponentRef, HandlerError> {
    let suffix = str_value(args.get("kind"))
        .filter(|kind| !kind.is_empty())
        .map(|kind| js_format!("({})", kind));
    Ok(Rc::new(RefCell::new(Text::new(
        format_code_index_call("references", str_value(args.get("name")), theme, suffix).map_err(HandlerError::from)?,
        0,
        0,
    ))))
}

/// `renderResult` shared by the code-index tools.
pub fn code_index_render_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    Rc::new(RefCell::new(Text::new(
        format_code_index_result(result, options, theme, context.show_images),
        0,
        0,
    )))
}

fn with_renderers(tool: Arc<dyn AgentTool>, render_call: ToolRenderer, prompt_snippet: &str) -> ToolDefinition {
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.prompt_snippet = Some(prompt_snippet.into());
    definition.approval_kind = Some(crate::core::extensions::types::ApprovalKind::ReadOnly);
    definition.render_call = Some(render_call);
    definition.render_result = Some(infallible_render_result(code_index_render_result));
    definition
}

/// `createOutlineToolDefinition`: outline plus renderers.
pub fn create_outline_tool_definition(cwd: impl Into<JsString>, options: CodeIndexToolOptions) -> ToolDefinition {
    with_renderers(
        Arc::new(OutlineTool::with_options(cwd, options)),
        Arc::new(outline_render_call),
        "Show structural outlines for files or directories",
    )
}

/// `createSymbolToolDefinition`: symbol plus renderers.
pub fn create_symbol_tool_definition(cwd: impl Into<JsString>, options: CodeIndexToolOptions) -> ToolDefinition {
    with_renderers(
        Arc::new(SymbolTool::with_options(cwd, options)),
        Arc::new(symbol_render_call),
        "Find symbol definitions by name",
    )
}

/// `createReferencesToolDefinition`: references plus renderers.
pub fn create_references_tool_definition(cwd: impl Into<JsString>, options: CodeIndexToolOptions) -> ToolDefinition {
    with_renderers(
        Arc::new(ReferencesTool::with_options(cwd, options)),
        Arc::new(references_render_call),
        "Find structural references to a symbol",
    )
}

/// `createOutlineTool`: outline wrapped with the shared result cap.
pub fn create_outline_tool(cwd: impl Into<JsString>, options: CodeIndexToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(OutlineTool::with_options(cwd, options)))
}

/// `createSymbolTool`: symbol wrapped with the shared result cap.
pub fn create_symbol_tool(cwd: impl Into<JsString>, options: CodeIndexToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(SymbolTool::with_options(cwd, options)))
}

/// `createReferencesTool`: references wrapped with the shared result cap.
pub fn create_references_tool(cwd: impl Into<JsString>, options: CodeIndexToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(ReferencesTool::with_options(cwd, options)))
}
