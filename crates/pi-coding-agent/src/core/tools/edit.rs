//! Ported from `packages/coding-agent/src/core/tools/edit.ts`.
//!
//! Queued exact-text replacements with fuzzy/CRLF handling delegated to the
//! shared `edit_diff` module.

use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_tui::tui::{ComponentRef, Container};
use pi_tui::{Box as BoxComponent, Spacer, Text};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use super::edit_diff::{
    Edit, apply_edits_to_normalized_content, detect_line_ending, generate_diff_string, generate_unified_patch,
    normalize_to_lf, restore_line_endings, strip_bom,
};
use super::file_mutation_queue::with_file_mutation_queue;
use super::node_fs::node_error_code;
use super::path_utils::resolve_to_cwd;
use super::render_utils::{render_tool_path, str_value};
use super::tool_definition_wrapper::create_tool_definition_from_agent_tool;
use crate::core::extensions::types::{RenderShell, ToolDefinition, ToolRenderContext, ToolRenderResultOptions};
use crate::modes::interactive::components::diff::render_diff;
use crate::modes::interactive::theme::{Theme, ThemeBg, ThemeColor};

const DESCRIPTION: &str = "Edit a single file using exact text replacement. Every edits[].oldText must match a unique, non-overlapping region of the original file. If two changes affect the same block or nearby lines, merge them into one edit instead of emitting overlapping edits. Do not include large unchanged regions just to connect distant changes.";
const DIFF_CONTEXT_LINES: usize = 4;

/// File access used by [`EditTool`]. Override to delegate edits to a remote system.
pub trait EditOperations: Send + Sync {
    fn read_file<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<Vec<u8>>>;
    fn write_file<'a>(&'a self, absolute_path: &'a str, content: &'a str) -> BoxFuture<'a, io::Result<()>>;
    fn access<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<()>>;
}

/// Local filesystem operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalEditOperations;

impl EditOperations for LocalEditOperations {
    fn read_file<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async move { tokio::fs::read(absolute_path).await })
    }

    fn write_file<'a>(&'a self, absolute_path: &'a str, content: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move { tokio::fs::write(absolute_path, content).await })
    }

    fn access<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            let path = std::ffi::CString::new(absolute_path)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains a NUL byte"))?;
            rustix::fs::access(
                path.as_c_str(),
                rustix::fs::Access::READ_OK | rustix::fs::Access::WRITE_OK,
            )
            .map_err(io::Error::from)
        })
    }
}

/// Options for [`EditTool`].
#[derive(Clone, Default)]
pub struct EditToolOptions {
    /// Custom file access. Default: local filesystem.
    pub operations: Option<Arc<dyn EditOperations>>,
}

/// Edits a single file with exact-text replacements.
pub struct EditTool {
    cwd: String,
    parameters: Value,
    operations: Arc<dyn EditOperations>,
}

impl EditTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_options(cwd, EditToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<String>, options: EditToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: edit_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalEditOperations)),
        }
    }
}

fn edit_schema() -> Value {
    json!({
        "type": "object",
        "required": ["path", "edits"],
        "properties": {
            "path": {"type": "string", "description": "Path to the file to edit (relative or absolute)"},
            "edits": {
                "type": "array",
                "description": "One or more targeted replacements. Each edit is matched against the original file, not incrementally. Do not include overlapping or nested edits. If two changes touch the same block or nearby lines, merge them into one edit instead.",
                "items": {
                    "type": "object",
                    "required": ["oldText", "newText"],
                    "properties": {
                        "oldText": {"type": "string", "description": "Exact text for one targeted replacement. It must be unique in the original file and must not overlap with any other edits[].oldText in the same call."},
                        "newText": {"type": "string", "description": "Replacement text for this targeted edit."}
                    }
                }
            }
        }
    })
}

impl AgentTool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "edit"
    }

    fn prepare_arguments(&self, args: Value) -> Value {
        prepare_edit_arguments(args)
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
        Box::pin(async move { execute_edit(&cwd, operations, params, signal).await })
    }
}

/// Compatibility shim for models that send `edits` as a JSON string or legacy
/// top-level `oldText`/`newText` fields.
pub fn prepare_edit_arguments(input: Value) -> Value {
    let Value::Object(mut args) = input else {
        return input;
    };

    if let Some(Value::String(encoded)) = args.get("edits")
        && let Ok(Value::Array(parsed)) = serde_json::from_str::<Value>(encoded)
    {
        args.insert("edits".to_owned(), Value::Array(parsed));
    }

    let legacy = match (
        args.get("oldText").and_then(Value::as_str),
        args.get("newText").and_then(Value::as_str),
    ) {
        (Some(old_text), Some(new_text)) => Some((old_text.to_owned(), new_text.to_owned())),
        _ => None,
    };
    let Some((old_text, new_text)) = legacy else {
        return Value::Object(args);
    };

    let mut edits = match args.remove("edits") {
        Some(Value::Array(edits)) => edits,
        _ => Vec::new(),
    };
    edits.push(json!({"oldText": old_text, "newText": new_text}));
    args.remove("oldText");
    args.remove("newText");
    args.insert("edits".to_owned(), Value::Array(edits));
    Value::Object(args)
}

fn parse_edit_input(input: &Value) -> Result<(String, Vec<Edit>), ToolError> {
    let edits = match input.get("edits") {
        Some(Value::Array(edits)) if !edits.is_empty() => edits,
        _ => {
            return Err(ToolError::new(
                "Edit tool input is invalid. edits must contain at least one replacement.",
            ));
        }
    };
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("edit tool requires a string path"))?
        .to_owned();
    let edits = edits
        .iter()
        .map(|edit| Edit {
            old_text: edit
                .get("oldText")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            new_text: edit
                .get("newText")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        })
        .collect();
    Ok((path, edits))
}

async fn execute_edit(
    cwd: &str,
    operations: Arc<dyn EditOperations>,
    params: Value,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let (path, edits) = parse_edit_input(&params)?;
    let absolute_path = resolve_to_cwd(&path, cwd).map_err(|error| ToolError::new(error.to_string()))?;
    with_file_mutation_queue(&absolute_path, || async {
        check_aborted(signal.as_ref())?;
        if let Err(error) = operations.access(&absolute_path).await {
            check_aborted(signal.as_ref())?;
            return Err(ToolError::new(format!(
                "Could not edit file: {path}. Error code: {}.",
                node_error_code(&error)
            )));
        }
        check_aborted(signal.as_ref())?;

        let buffer = operations
            .read_file(&absolute_path)
            .await
            .map_err(|error| ToolError::new(super::node_fs::node_fs_error(&error, "open", &absolute_path)))?;
        check_aborted(signal.as_ref())?;

        let raw_content = String::from_utf8_lossy(&buffer);
        let bom = strip_bom(&raw_content);
        let original_ending = detect_line_ending(&bom.text);
        let normalized_content = normalize_to_lf(&bom.text);
        let applied = apply_edits_to_normalized_content(&normalized_content, &edits, &path)
            .map_err(|error| ToolError::new(error.to_string()))?;
        check_aborted(signal.as_ref())?;

        let final_content = format!(
            "{}{}",
            bom.bom,
            restore_line_endings(&applied.new_content, original_ending)
        );
        operations
            .write_file(&absolute_path, &final_content)
            .await
            .map_err(|error| ToolError::new(super::node_fs::node_fs_error(&error, "open", &absolute_path)))?;
        check_aborted(signal.as_ref())?;

        let diff_result = generate_diff_string(&applied.base_content, &applied.new_content, DIFF_CONTEXT_LINES);
        let patch = generate_unified_patch(&path, &applied.base_content, &applied.new_content, DIFF_CONTEXT_LINES);
        let mut details = Map::new();
        details.insert("diff".to_owned(), Value::String(diff_result.diff));
        details.insert("patch".to_owned(), Value::String(patch));
        if let Some(first_changed_line) = diff_result.first_changed_line {
            details.insert("firstChangedLine".to_owned(), json!(first_changed_line));
        }
        Ok(AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(format!(
                "Successfully replaced {} block(s) in {path}.",
                edits.len()
            )))],
            details: Some(Value::Object(details)),
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
    str_value(args.get("file_path").or_else(|| args.get("path")))
}

fn format_edit_call(args: &Value, theme: &Theme, cwd: &str) -> String {
    let path_display = render_tool_path(render_args_path(args).as_deref(), theme, cwd, None);
    format!(
        "{} {path_display}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("edit"))
    )
}

fn format_edit_result(_args: &Value, result: &AgentToolResult, theme: &Theme, is_error: bool) -> Option<String> {
    if is_error {
        let error_text = result
            .content
            .iter()
            .filter_map(|entry| match entry {
                TextOrImageContent::Text(text) => Some(text.text.as_str()),
                TextOrImageContent::Image(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        if error_text.is_empty() {
            return None;
        }
        return Some(theme.fg(ThemeColor::Error, &error_text));
    }

    let result_diff = result
        .details
        .as_ref()
        .and_then(|details| details.get("diff"))
        .and_then(Value::as_str)
        .filter(|diff| !diff.is_empty())?;
    Some(render_diff(result_diff))
}

/// `renderCall` for the edit tool.
pub fn render_call(args: &Value, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    let theme = Arc::new(theme.clone());
    let background_theme = Arc::clone(&theme);
    let mut component = BoxComponent::new(
        1,
        1,
        Some(Rc::new(move |text: &str| {
            background_theme.bg(ThemeBg::ToolPendingBg, text)
        })),
    );
    component.add_child(Rc::new(RefCell::new(Text::new(
        format_edit_call(args, theme.as_ref(), &context.cwd),
        0,
        0,
    ))));
    Rc::new(RefCell::new(component))
}

/// `renderResult` for the edit tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    let mut component = Container::new();
    if let Some(output) = format_edit_result(&context.args, result, theme, context.is_error) {
        component.add_child(Rc::new(RefCell::new(Spacer::new(1))));
        component.add_child(Rc::new(RefCell::new(Text::new(output, 1, 0))));
    }
    Rc::new(RefCell::new(component))
}

/// `createEditToolDefinition`: the edit tool plus its renderers.
pub fn create_edit_tool_definition(cwd: &str, options: EditToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(EditTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.render_shell = Some(RenderShell::SelfRender);
    definition.render_call = Some(Arc::new(render_call));
    definition.render_result = Some(Arc::new(render_result));
    definition
}
