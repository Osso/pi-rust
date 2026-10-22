//! Ported from `packages/coding-agent/src/core/tools/write.ts`.
//! Queued whole-file writes with native UTF-16 paths and content.

use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_tui::Text;
use pi_tui::tui::ComponentRef;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::file_mutation_queue::with_file_mutation_queue;
use super::node_fs::node_fs_error;
use super::path_utils::resolve_to_cwd;
use super::render_utils::{normalize_display_text, render_tool_path, replace_tabs, str_value};
use super::tool_definition_wrapper::{create_tool_definition_from_agent_tool, wrap_agent_tool};
use crate::core::extensions::types::{
    ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_call, infallible_render_result,
};
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{Theme, ThemeColor, get_language_from_path, highlight_code};
use crate::utils::paths::node_dirname;

const DESCRIPTION: &str = "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories.";

/// `writeToolSystemPromptContribution`.
pub const WRITE_TOOL_SYSTEM_PROMPT_CONTRIBUTION: super::ToolSystemPromptContribution =
    super::ToolSystemPromptContribution {
        snippet: "Create or overwrite files",
        guidelines: &["Use write only for new files or complete rewrites."],
    };

/// File writes used by [`WriteTool`]. Custom operations receive original JS units.
pub trait WriteOperations: Send + Sync {
    fn write_file<'a>(&'a self, absolute_path: &'a JsString, content: &'a JsString) -> BoxFuture<'a, io::Result<()>>;
    fn mkdir<'a>(&'a self, directory: &'a JsString) -> BoxFuture<'a, io::Result<()>>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LocalWriteOperations;

impl WriteOperations for LocalWriteOperations {
    fn write_file<'a>(&'a self, absolute_path: &'a JsString, content: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            super::node_fs::validate_node_path(absolute_path)?;
            tokio::fs::write(encode_node_utf8(absolute_path), encode_node_utf8(content))
                .await
                .map_err(|error| super::node_fs::operation_error(node_fs_error(&error, "open", absolute_path)))
        })
    }

    fn mkdir<'a>(&'a self, directory: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            super::node_fs::validate_node_path(directory)?;
            tokio::fs::create_dir_all(encode_node_utf8(directory))
                .await
                .map_err(|error| super::node_fs::operation_error(node_fs_error(&error, "mkdir", directory)))
        })
    }
}

#[derive(Clone, Default)]
pub struct WriteToolOptions {
    pub operations: Option<Arc<dyn WriteOperations>>,
}

pub struct WriteTool {
    cwd: JsString,
    parameters: Value,
    operations: Arc<dyn WriteOperations>,
}

impl WriteTool {
    pub fn new(cwd: impl Into<JsString>) -> Self {
        Self::with_options(cwd, WriteToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<JsString>, options: WriteToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: write_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalWriteOperations)),
        }
    }
}

fn write_schema() -> Value {
    json!({
        "type": "object", "required": ["path", "content"],
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
    fn constrained_sampling(&self) -> Option<pi_ai::types::ConstrainedSampling> {
        crate::core::experimental::get_experimental_tool_constrained_sampling()
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
        Box::pin(async move { execute_write(&cwd, operations, params, signal).await })
    }
}

async fn execute_write(
    cwd: &JsString,
    operations: Arc<dyn WriteOperations>,
    params: JsValue,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let path = params
        .get("path")
        .and_then(JsValue::as_string)
        .ok_or_else(|| ToolError::new("write tool requires a string path"))?;
    let content = params
        .get("content")
        .and_then(JsValue::as_string)
        .ok_or_else(|| ToolError::new("write tool requires a string content"))?;
    let absolute_path = resolve_to_cwd(path, cwd).map_err(|error| ToolError::new(error.message().clone()))?;
    let directory = node_dirname(&absolute_path);
    with_file_mutation_queue(&absolute_path, || async {
        check_aborted(signal.as_ref())?;
        operations
            .mkdir(&directory)
            .await
            .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?;
        check_aborted(signal.as_ref())?;
        operations
            .write_file(&absolute_path, content)
            .await
            .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?;
        check_aborted(signal.as_ref())?;
        Ok(AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(js_format!(
                "Successfully wrote {} bytes to {}",
                content.len().to_string(),
                path
            )))],
            details: None,
            is_error: None,
            terminate: None,
        })
    })
    .await
    .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?
}

fn check_aborted(signal: Option<&CancellationToken>) -> Result<(), ToolError> {
    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Err(ToolError::new("Operation aborted"));
    }
    Ok(())
}

fn render_args_path(args: &JsValue) -> Option<JsString> {
    let value = args.get("file_path").filter(|value| !value.is_null());
    str_value(value.or_else(|| args.get("path")))
}

fn trim_trailing_empty_lines(mut lines: Vec<JsString>) -> Vec<JsString> {
    while lines.last().is_some_and(JsString::is_empty) {
        lines.pop();
    }
    lines
}

fn format_write_call(args: &JsValue, options: &ToolRenderResultOptions, theme: &Theme, cwd: &JsString) -> JsString {
    let raw_path = render_args_path(args);
    let path_display = render_tool_path(raw_path.as_ref(), theme, cwd, None);
    let mut text = js_format!(
        "{} {}",
        theme.fg(ThemeColor::ToolTitle, theme.bold("write")),
        path_display
    );
    let Some(file_content) = str_value(args.get("content")) else {
        text.push_str(js_format!(
            "\n\n{}",
            theme.fg(ThemeColor::Error, "[invalid content arg - expected string]")
        ));
        return text;
    };
    if file_content.is_empty() {
        return text;
    }
    let lang = raw_path.as_ref().and_then(get_language_from_path).map(JsString::from);
    let normalized = normalize_display_text(&file_content);
    let rendered_lines = match lang.as_ref() {
        Some(lang) => highlight_code(&replace_tabs(&normalized), Some(lang)),
        None => normalized.split('\n').collect(),
    };
    let lines = trim_trailing_empty_lines(rendered_lines);
    append_write_preview(&mut text, &lines, lang.is_some(), options.expanded, theme);
    text
}

fn append_write_preview(text: &mut JsString, lines: &[JsString], highlighted: bool, expanded: bool, theme: &Theme) {
    let total_lines = lines.len();
    let max_lines = if expanded { total_lines } else { 10 };
    let display = JsString::join(
        lines.iter().take(max_lines).map(|line| {
            if highlighted {
                line.clone()
            } else {
                theme.fg(ThemeColor::ToolOutput, replace_tabs(line))
            }
        }),
        "\n",
    );
    text.push_str(js_format!("\n\n{}", display));
    let remaining = total_lines.saturating_sub(max_lines);
    if remaining > 0 {
        text.push_str(js_format!(
            "{}{}{}",
            theme.fg(
                ThemeColor::Muted,
                format!("\n... ({remaining} more lines, {total_lines} total,")
            ),
            key_hint("app.tools.expand", "to expand"),
            theme.fg(ThemeColor::Muted, ")")
        ));
    }
}

fn format_write_result(result: &AgentToolResult, theme: &Theme, is_error: bool) -> Option<JsString> {
    if !is_error {
        return None;
    }
    let output = JsString::join(
        result.content.iter().filter_map(|entry| match entry {
            TextOrImageContent::Text(text) => Some(&text.text),
            TextOrImageContent::Image(_) => None,
        }),
        "\n",
    );
    if output.is_empty() {
        return None;
    }
    Some(js_format!("\n{}", theme.fg(ThemeColor::Error, output)))
}

pub fn render_call(args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
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

pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    match format_write_result(result, theme, context.is_error) {
        Some(text) => Rc::new(RefCell::new(Text::new(text, 0, 0))),
        None => Rc::new(RefCell::new(pi_tui::Container::new())),
    }
}

pub fn create_write_tool_definition(cwd: impl Into<JsString>, options: WriteToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(WriteTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.prompt_snippet = Some(WRITE_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet.into());
    definition.prompt_guidelines = Some(
        WRITE_TOOL_SYSTEM_PROMPT_CONTRIBUTION
            .guidelines
            .iter()
            .map(|line| (*line).into())
            .collect(),
    );
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

pub fn create_write_tool(cwd: impl Into<JsString>, options: WriteToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(WriteTool::with_options(cwd, options)))
}
