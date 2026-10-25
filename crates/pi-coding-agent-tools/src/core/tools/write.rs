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
use pi_js_compat::abort::AbortSignal;
use pi_tui::tui::ComponentRef;
use pi_tui::{Component, Text};
use serde_json::{Value, json};

use super::file_mutation_queue::with_file_mutation_queue;
use super::node_fs::node_fs_error;
use super::path_utils::resolve_to_cwd;
use super::render_utils::{normalize_display_text, render_tool_path, replace_tabs, str_value};
use super::tool_definition_wrapper::{create_tool_definition_from_agent_tool, wrap_agent_tool};
use crate::core::extensions::types::{HandlerError, ToolDefinition, ToolRenderContext, ToolRenderResultOptions};
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{HighlightLanguage, Theme, ThemeColor, get_language_from_path, highlight_code};
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
    fn description(&self) -> pi_js_compat::js_json::JsString {
        DESCRIPTION.into()
    }
    fn parameters(&self) -> &Value {
        &self.parameters
    }
    fn label(&self) -> pi_js_compat::js_json::JsString {
        "write".into()
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
        Box::pin(async move { execute_write(&cwd, operations, params, signal).await })
    }
}

async fn execute_write(
    cwd: &JsString,
    operations: Arc<dyn WriteOperations>,
    params: JsValue,
    signal: Option<AbortSignal>,
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

fn check_aborted(signal: Option<&AbortSignal>) -> Result<(), ToolError> {
    if signal.is_some_and(AbortSignal::aborted) {
        return Err(ToolError::new("Operation aborted"));
    }
    Ok(())
}

const WRITE_PARTIAL_FULL_HIGHLIGHT_LINES: usize = 50;

struct WriteHighlightCache {
    raw_path: Option<JsString>,
    lang: HighlightLanguage,
    raw_content: JsString,
    normalized_lines: Vec<JsString>,
    highlighted_lines: Vec<JsString>,
}

struct WriteCallRenderComponent {
    text: Text,
    cache: Option<WriteHighlightCache>,
}

impl WriteCallRenderComponent {
    fn new() -> Self {
        Self {
            text: Text::new("", 0, 0),
            cache: None,
        }
    }

    fn update(&mut self, args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> Result<(), HandlerError> {
        let raw_path = render_args_path(args);
        match str_value(args.get("content")) {
            None => self.cache = None,
            Some(content) if context.args_complete => {
                self.cache = rebuild_write_highlight_cache_full(raw_path, &content)?
            }
            Some(content) => update_write_highlight_cache_incremental(&mut self.cache, raw_path, &content)?,
        }
        let options = ToolRenderResultOptions {
            expanded: context.expanded,
            is_partial: context.is_partial,
        };
        self.text.set_text(format_write_call(
            args,
            &options,
            theme,
            self.cache.as_ref(),
            &context.cwd,
        )?);
        Ok(())
    }
}

impl Component for WriteCallRenderComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.text.render(width)
    }
    fn invalidate(&mut self) {
        self.text.invalidate();
    }
    fn wants_input(&self) -> bool {
        false
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

fn highlight_single_line(line: &JsString, lang: &HighlightLanguage) -> Result<JsString, HandlerError> {
    Ok(highlight_code(line, Some(lang))?.into_iter().next().unwrap_or_default())
}

fn refresh_write_highlight_prefix(cache: &mut WriteHighlightCache) -> Result<(), HandlerError> {
    let count = WRITE_PARTIAL_FULL_HIGHLIGHT_LINES.min(cache.normalized_lines.len());
    if count == 0 {
        return Ok(());
    }
    let prefix = JsString::join(&cache.normalized_lines[..count], "\n");
    let highlighted = highlight_code(&prefix, Some(&cache.lang))?;
    for index in 0..count {
        cache.highlighted_lines[index] = match highlighted.get(index) {
            Some(line) => line.clone(),
            None => highlight_single_line(&cache.normalized_lines[index], &cache.lang)?,
        };
    }
    Ok(())
}

fn rebuild_write_highlight_cache_full(
    raw_path: Option<JsString>,
    content: &JsString,
) -> Result<Option<WriteHighlightCache>, HandlerError> {
    let Some(lang) = raw_path.as_ref().and_then(get_language_from_path) else {
        return Ok(None);
    };
    let normalized = replace_tabs(&normalize_display_text(content));
    let highlighted_lines = highlight_code(&normalized, Some(&lang))?;
    Ok(Some(WriteHighlightCache {
        raw_path,
        lang,
        raw_content: content.clone(),
        normalized_lines: normalized.split('\n').collect(),
        highlighted_lines,
    }))
}

fn update_write_highlight_cache_incremental(
    cache: &mut Option<WriteHighlightCache>,
    raw_path: Option<JsString>,
    content: &JsString,
) -> Result<(), HandlerError> {
    let Some(lang) = raw_path.as_ref().and_then(get_language_from_path) else {
        *cache = None;
        return Ok(());
    };
    let Some(existing) = cache.as_mut() else {
        *cache = rebuild_write_highlight_cache_full(raw_path, content)?;
        return Ok(());
    };
    if existing.lang != lang || existing.raw_path != raw_path || !content.starts_with(&existing.raw_content) {
        *cache = rebuild_write_highlight_cache_full(raw_path, content)?;
        return Ok(());
    }
    if content.len() == existing.raw_content.len() {
        return Ok(());
    }
    let delta = content.slice(existing.raw_content.len(), content.len());
    existing.raw_content = content.clone();
    append_write_highlight_delta(existing, &delta)
}

fn append_write_highlight_delta(cache: &mut WriteHighlightCache, delta: &JsString) -> Result<(), HandlerError> {
    let normalized = replace_tabs(&normalize_display_text(delta));
    if cache.normalized_lines.is_empty() {
        cache.normalized_lines.push(JsString::default());
        cache.highlighted_lines.push(JsString::default());
    }
    let mut segments = normalized.split('\n');
    let last_index = cache.normalized_lines.len() - 1;
    cache.normalized_lines[last_index].push_str(segments.next().expect("split yields a segment"));
    cache.highlighted_lines[last_index] = highlight_single_line(&cache.normalized_lines[last_index], &cache.lang)?;
    for segment in segments {
        cache.normalized_lines.push(segment.clone());
        cache
            .highlighted_lines
            .push(highlight_single_line(&segment, &cache.lang)?);
    }
    refresh_write_highlight_prefix(cache)
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

fn format_write_call(
    args: &JsValue,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    cache: Option<&WriteHighlightCache>,
    cwd: &JsString,
) -> Result<JsString, HandlerError> {
    let raw_path = render_args_path(args);
    let path_display = render_tool_path(raw_path.as_ref(), theme, cwd, None)?;
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
        return Ok(text);
    };
    if file_content.is_empty() {
        return Ok(text);
    }
    let lang = raw_path.as_ref().and_then(get_language_from_path);
    let rendered_lines = match lang.as_ref() {
        Some(lang) => match cache {
            Some(cache) => cache.highlighted_lines.clone(),
            None => highlight_code(&replace_tabs(&normalize_display_text(&file_content)), Some(lang))?,
        },
        None => normalize_display_text(&file_content).split('\n').collect(),
    };
    let lines = trim_trailing_empty_lines(rendered_lines);
    append_write_preview(&mut text, &lines, lang.is_some(), options.expanded, theme);
    Ok(text)
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
            "{} {}{}",
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

pub fn render_call(args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> Result<ComponentRef, HandlerError> {
    if let Some(component) = &context.last_component
        && let Some(write) = component
            .borrow_mut()
            .as_any_mut()
            .and_then(|component| component.downcast_mut::<WriteCallRenderComponent>())
    {
        write.update(args, theme, context)?;
        return Ok(Rc::clone(component));
    }
    let mut component = WriteCallRenderComponent::new();
    component.update(args, theme, context)?;
    Ok(Rc::new(RefCell::new(component)))
}

pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> Result<ComponentRef, HandlerError> {
    let output = format_write_result(result, theme, context.is_error);
    let component = context.last_component.clone().unwrap_or_else(|| {
        if output.is_some() {
            Rc::new(RefCell::new(Text::new("", 0, 0)))
        } else {
            Rc::new(RefCell::new(pi_tui::Container::new()))
        }
    });
    update_write_result_component(&component, output)?;
    Ok(component)
}

fn update_write_result_component(component: &ComponentRef, output: Option<JsString>) -> Result<(), HandlerError> {
    let mut component = component.borrow_mut();
    let native = component.as_any_mut();
    match output {
        Some(output) => {
            let text = native
                .and_then(|component| component.downcast_mut::<Text>())
                .ok_or_else(|| write_result_type_error("text.setText is not a function"))?;
            text.set_text(output);
        }
        None => {
            let container = native
                .and_then(|component| component.downcast_mut::<pi_tui::Container>())
                .ok_or_else(|| write_result_type_error("component.clear is not a function"))?;
            container.clear();
        }
    }
    Ok(())
}

fn write_result_type_error(message: &str) -> HandlerError {
    pi_js_compat::node_error::NativeNodeError::from_constructor(
        pi_js_compat::node_error::NativeErrorClass::TypeError,
        message,
    )
    .into()
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
    definition.render_call = Some(std::sync::Arc::new(render_call));
    definition.render_result = Some(Arc::new(render_result));
    definition
}

pub fn create_write_tool(cwd: impl Into<JsString>, options: WriteToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(WriteTool::with_options(cwd, options)))
}
