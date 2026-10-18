//! Ported from `packages/coding-agent/src/core/tools/write.ts`.
//!
//! Queued whole-file writes with parent-directory creation. Renderers are
//! pending `theme.ts`.

use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::string::utf16_len;
use pi_tui::Text;
use pi_tui::tui::ComponentRef;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::file_mutation_queue::with_file_mutation_queue;
use super::node_fs::node_fs_error;
use super::path_utils::resolve_to_cwd;
use super::render_utils::{key_hint, normalize_display_text, render_tool_path, replace_tabs, str_value};
use super::tool_definition_wrapper::wrap_agent_tool;
use crate::core::extensions::types::{ExtensionTheme, ToolDefinition, ToolRenderContext, ToolRenderResultOptions};
use crate::core::extensions::wrapper::create_tool_definition_from_agent_tool;
use crate::modes::interactive::theme::{get_language_from_path, highlight_code};

const DESCRIPTION: &str = "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories.";

/// File writes used by [`WriteTool`]. Override to delegate writes to a remote system.
pub trait WriteOperations: Send + Sync {
    fn write_file<'a>(&'a self, absolute_path: &'a str, content: &'a str) -> BoxFuture<'a, io::Result<()>>;
    fn mkdir<'a>(&'a self, directory: &'a str) -> BoxFuture<'a, io::Result<()>>;
}

/// Local filesystem operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalWriteOperations;

impl WriteOperations for LocalWriteOperations {
    fn write_file<'a>(&'a self, absolute_path: &'a str, content: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move { tokio::fs::write(absolute_path, content).await })
    }

    fn mkdir<'a>(&'a self, directory: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move { tokio::fs::create_dir_all(directory).await })
    }
}

/// Options for [`WriteTool`].
#[derive(Clone, Default)]
pub struct WriteToolOptions {
    /// Custom file writes. Default: local filesystem.
    pub operations: Option<Arc<dyn WriteOperations>>,
}

/// Writes whole files.
pub struct WriteTool {
    cwd: String,
    parameters: Value,
    operations: Arc<dyn WriteOperations>,
}

impl WriteTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_options(cwd, WriteToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<String>, options: WriteToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: write_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalWriteOperations)),
        }
    }
}

fn write_schema() -> Value {
    json!({
        "type": "object",
        "required": ["path", "content"],
        "properties": {
            "path": {"type": "string", "description": "Path to the file to write (relative or absolute)"},
            "content": {"type": "string", "description": "Content to write to the file"}
        }
    })
}

impl AgentTool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "write"
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
        Box::pin(async move { execute_write(&cwd, operations, params, signal).await })
    }
}

async fn execute_write(
    cwd: &str,
    operations: Arc<dyn WriteOperations>,
    params: Value,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("write tool requires a string path"))?
        .to_owned();
    let content = params
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("write tool requires a string content"))?
        .to_owned();
    let absolute_path = resolve_to_cwd(&path, cwd).map_err(|error| ToolError::new(error.to_string()))?;
    let directory = Path::new(&absolute_path)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".to_owned());

    with_file_mutation_queue(&absolute_path, || async {
        check_aborted(signal.as_ref())?;
        operations
            .mkdir(&directory)
            .await
            .map_err(|error| ToolError::new(node_fs_error(&error, "mkdir", &directory)))?;
        check_aborted(signal.as_ref())?;
        operations
            .write_file(&absolute_path, &content)
            .await
            .map_err(|error| ToolError::new(node_fs_error(&error, "open", &absolute_path)))?;
        check_aborted(signal.as_ref())?;
        Ok(AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(format!(
                "Successfully wrote {} bytes to {path}",
                utf16_len(&content)
            )))],
            details: None,
            usage: None,
            added_tool_names: None,
            is_error: None,
            terminate: None,
        })
    })
    .await
    .map_err(|error| ToolError::new(error.to_string()))?
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

fn render_args_path(args: &Value) -> Option<String> {
    let value = args.get("file_path").filter(|value| !value.is_null());
    str_value(value.or_else(|| args.get("path")))
}

fn render_content(args: &Value) -> Option<String> {
    str_value(args.get("content"))
}

fn trim_trailing_empty_lines(mut lines: Vec<String>) -> Vec<String> {
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

fn format_write_call(args: &Value, options: &ToolRenderResultOptions, theme: &dyn ExtensionTheme, cwd: &str) -> String {
    let raw_path = render_args_path(args);
    let file_content = render_content(args);
    let path_display = render_tool_path(raw_path.as_deref(), theme, cwd, None);
    let mut text = format!("{} {path_display}", theme.fg("toolTitle", &theme.bold("write")));
    let Some(file_content) = file_content else {
        text.push_str(&format!(
            "\n\n{}",
            theme.fg("error", "[invalid content arg - expected string]")
        ));
        return text;
    };
    if file_content.is_empty() {
        return text;
    }
    let lang = raw_path.as_deref().and_then(get_language_from_path);
    let rendered_lines = match lang {
        Some(lang) => highlight_code(&replace_tabs(&normalize_display_text(&file_content)), Some(lang)),
        None => normalize_display_text(&file_content)
            .split('\n')
            .map(str::to_owned)
            .collect(),
    };
    let lines = trim_trailing_empty_lines(rendered_lines);
    let total_lines = lines.len();
    let max_lines = if options.expanded { total_lines } else { 10 };
    let display_lines = lines.iter().take(max_lines).collect::<Vec<_>>();
    let remaining = total_lines.saturating_sub(max_lines);
    text.push_str(&format!(
        "\n\n{}",
        display_lines
            .iter()
            .map(|line| match lang {
                Some(_) => (*line).clone(),
                None => theme.fg("toolOutput", &replace_tabs(line)),
            })
            .collect::<Vec<_>>()
            .join("\n")
    ));
    if remaining > 0 {
        text.push_str(&format!(
            "{}{}{}",
            theme.fg("muted", &format!("\n... ({remaining} more lines, {total_lines} total,")),
            key_hint("app.tools.expand", "to expand"),
            theme.fg("muted", ")")
        ));
    }
    text
}

fn format_write_result(result: &AgentToolResult, theme: &dyn ExtensionTheme, is_error: bool) -> Option<String> {
    if !is_error {
        return None;
    }
    let output = result
        .content
        .iter()
        .filter_map(|entry| match entry {
            TextOrImageContent::Text(text) => Some(text.text.as_str()),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    if output.is_empty() {
        return None;
    }
    Some(format!("\n{}", theme.fg("error", &output)))
}

/// `renderCall` for the write tool.
pub fn render_call(args: &Value, theme: &dyn ExtensionTheme, context: &ToolRenderContext) -> ComponentRef {
    let options = ToolRenderResultOptions {
        expanded: context.expanded,
        is_partial: context.is_partial,
    };
    Rc::new(RefCell::new(Text::new(
        format_write_call(args, &options, theme, &context.cwd),
        0,
        0,
    )))
}

/// `renderResult` for the write tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &dyn ExtensionTheme,
    context: &ToolRenderContext,
) -> ComponentRef {
    match format_write_result(result, theme, context.is_error) {
        Some(text) => Rc::new(RefCell::new(Text::new(text, 0, 0))),
        None => Rc::new(RefCell::new(pi_tui::Container::new())),
    }
}

/// `createWriteToolDefinition`: the write tool plus its renderers.
pub fn create_write_tool_definition(cwd: &str, options: WriteToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(WriteTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.render_call = Some(Arc::new(render_call));
    definition.render_result = Some(Arc::new(render_result));
    definition
}

/// `createWriteTool`: the write tool wrapped with the shared result cap.
pub fn create_write_tool(cwd: &str, options: WriteToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(WriteTool::with_options(cwd, options)))
}
