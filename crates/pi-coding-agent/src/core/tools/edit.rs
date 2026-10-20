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
    Edit, EditDiffOutcome, apply_edits_to_normalized_content, compute_edits_diff, detect_line_ending,
    generate_diff_string, generate_unified_patch, normalize_to_lf, restore_line_endings, strip_bom,
};
use super::file_mutation_queue::with_file_mutation_queue;
use super::path_utils::resolve_to_cwd;
use super::render_utils::render_tool_path;
use super::tool_definition_wrapper::{create_tool_definition_from_agent_tool, wrap_agent_tool};
use crate::core::extensions::types::{
    HandlerError, RenderShell, ToolDefinition, ToolRenderContext, ToolRenderResultOptions,
};
use crate::modes::interactive::components::diff::render_diff;
use crate::modes::interactive::theme::{Theme, ThemeBg, ThemeColor};
use crate::utils::child_process::node_error_code;

const DESCRIPTION: &str = "Edit a single file using exact text replacement. Every edits[].oldText must match a unique, non-overlapping region of the original file. If two changes affect the same block or nearby lines, merge them into one edit instead of emitting overlapping edits. Do not include large unchanged regions just to connect distant changes.";
const DIFF_CONTEXT_LINES: usize = 4;

/// `editToolSystemPromptContribution`.
pub const EDIT_TOOL_SYSTEM_PROMPT_CONTRIBUTION: super::ToolSystemPromptContribution =
    super::ToolSystemPromptContribution {
        snippet: "Make precise file edits with exact text replacement, including multiple disjoint edits in one call",
        guidelines: &[
            "Use edit for precise changes (edits[].oldText must match exactly)",
            "When changing multiple separate locations in one file, use one edit call with multiple entries in edits[] instead of multiple edit calls",
            "Each edits[].oldText is matched against the original file, not after earlier edits are applied. Do not emit overlapping or nested edits. Merge nearby changes into one edit.",
            "Keep edits[].oldText as small as possible while still being unique in the file. Do not pad with large unchanged regions.",
        ],
    };

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
        Box::pin(async move {
            tokio::fs::read(absolute_path)
                .await
                .map_err(|error| io::Error::other(super::node_fs::node_fs_error(&error, "open", absolute_path)))
        })
    }

    fn write_file<'a>(&'a self, absolute_path: &'a str, content: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            tokio::fs::write(absolute_path, content)
                .await
                .map_err(|error| io::Error::other(super::node_fs::node_fs_error(&error, "open", absolute_path)))
        })
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

    fn constrained_sampling(&self) -> Option<pi_ai::types::ConstrainedSampling> {
        crate::core::experimental::get_experimental_tool_constrained_sampling()
    }

    fn prepare_arguments(&self, args: Value) -> Result<Value, ToolError> {
        Ok(prepare_edit_arguments(args))
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
                "Could not edit file: {path}. {}.",
                edit_access_error_message(&error)
            )));
        }
        check_aborted(signal.as_ref())?;

        let buffer = operations
            .read_file(&absolute_path)
            .await
            .map_err(|error| ToolError::new(error.to_string()))?;
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
            .map_err(|error| ToolError::new(error.to_string()))?;
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

/// `error instanceof Error && "code" in error ? Error code: <code> : String(error)`.
///
/// Custom operations surface plain messages; filesystem errors carry an errno.
fn edit_access_error_message(error: &io::Error) -> String {
    if error.raw_os_error().is_some() || error.kind() != io::ErrorKind::Other {
        return format!("Error code: {}", node_error_code(error));
    }
    format!("Error: {error}")
}

// ============================================================================
// Renderers
// ============================================================================

const PREVIEW_KEY: &str = "preview";
const PREVIEW_ARGS_KEY: &str = "previewArgsKey";
const PREVIEW_PENDING_KEY: &str = "previewPending";
const SETTLED_ERROR_KEY: &str = "settledError";

/// `EditDiffResult | EditDiffError` preview shown in the call header.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EditPreview {
    Diff {
        diff: String,
        first_changed_line: Option<usize>,
    },
    Error(String),
}

fn preview_from_state(state: &Value) -> Option<EditPreview> {
    let preview = state.get(PREVIEW_KEY)?;
    match preview.get("error").and_then(Value::as_str) {
        Some(error) => Some(EditPreview::Error(error.to_owned())),
        None => Some(EditPreview::Diff {
            diff: preview.get("diff")?.as_str()?.to_owned(),
            first_changed_line: preview
                .get("firstChangedLine")
                .and_then(Value::as_u64)
                .map(|line| line as usize),
        }),
    }
}

fn preview_to_state(preview: &EditPreview) -> Value {
    match preview {
        EditPreview::Diff {
            diff,
            first_changed_line,
        } => json!({"diff": diff, "firstChangedLine": first_changed_line}),
        EditPreview::Error(error) => json!({"error": error}),
    }
}

fn preview_args_key_from_state(state: &Value) -> Option<String> {
    state.get(PREVIEW_ARGS_KEY).and_then(Value::as_str).map(str::to_owned)
}

fn render_args_path(args: &Value) -> Option<String> {
    match args.get("file_path") {
        Some(Value::Null) | None => args.get("path").and_then(Value::as_str).map(str::to_owned),
        Some(Value::String(path)) => Some(path.clone()),
        Some(_) => None,
    }
}

fn format_edit_call(args: &Value, theme: &Theme, cwd: &str) -> String {
    let path_display = render_tool_path(render_args_path(args).as_deref(), theme, cwd, None);
    format!(
        "{} {path_display}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("edit"))
    )
}

/// `getRenderablePreviewInput`: the path plus the edits that can be previewed.
fn get_renderable_preview_input(args: &Value) -> Option<(String, Vec<Edit>, Value)> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .or_else(|| args.get("file_path").and_then(Value::as_str))?
        .to_owned();

    if let Some(Value::Array(edits)) = args.get("edits")
        && !edits.is_empty()
        && edits.iter().all(|edit| {
            edit.get("oldText").is_some_and(Value::is_string) && edit.get("newText").is_some_and(Value::is_string)
        })
    {
        let parsed = edits
            .iter()
            .map(|edit| Edit {
                old_text: edit["oldText"].as_str().unwrap_or_default().to_owned(),
                new_text: edit["newText"].as_str().unwrap_or_default().to_owned(),
            })
            .collect();
        return Some((path, parsed, Value::Array(edits.clone())));
    }

    let old_text = args.get("oldText").and_then(Value::as_str)?;
    let new_text = args.get("newText").and_then(Value::as_str)?;
    let edits = vec![Edit {
        old_text: old_text.to_owned(),
        new_text: new_text.to_owned(),
    }];
    Some((path, edits, json!([{"oldText": old_text, "newText": new_text}])))
}

fn preview_args_key(path: &str, raw_edits: &Value) -> String {
    serde_json::to_string(&json!({"path": path, "edits": raw_edits})).expect("preview key must serialize")
}

fn edit_header_bg(preview: Option<&EditPreview>, settled_error: bool) -> ThemeBg {
    match preview {
        Some(EditPreview::Error(_)) => ThemeBg::ToolErrorBg,
        Some(EditPreview::Diff { .. }) => ThemeBg::ToolSuccessBg,
        None if settled_error => ThemeBg::ToolErrorBg,
        None => ThemeBg::ToolPendingBg,
    }
}

/// `getEditCallRenderComponent`: reuse the last call component.
fn get_edit_call_component(context: &ToolRenderContext) -> ComponentRef {
    if let Some(last_component) = &context.last_component
        && last_component
            .borrow_mut()
            .as_any_mut()
            .and_then(|component| component.downcast_mut::<BoxComponent>())
            .is_some()
    {
        return Rc::clone(last_component);
    }
    Rc::new(RefCell::new(BoxComponent::new(1, 1, None)))
}

fn fill_edit_call_component(
    component: &ComponentRef,
    args: &Value,
    preview: Option<&EditPreview>,
    settled_error: bool,
    theme: &Theme,
    cwd: &str,
) {
    let background = edit_header_bg(preview, settled_error);
    let background_theme = theme.clone();
    let mut borrowed = component.borrow_mut();
    let Some(box_component) = borrowed
        .as_any_mut()
        .and_then(|component| component.downcast_mut::<BoxComponent>())
    else {
        return;
    };
    box_component.set_bg_fn(Some(Rc::new(move |text: &str| background_theme.bg(background, text))));
    box_component.clear();
    box_component.add_child(Rc::new(RefCell::new(Text::new(
        format_edit_call(args, theme, cwd),
        0,
        0,
    ))));

    let Some(preview) = preview else {
        return;
    };
    let body = match preview {
        EditPreview::Error(error) => theme.fg(ThemeColor::Error, error),
        EditPreview::Diff { diff, .. } => render_diff(diff),
    };
    box_component.add_child(Rc::new(RefCell::new(Spacer::new(1))));
    box_component.add_child(Rc::new(RefCell::new(Text::new(body, 0, 0))));
}

fn set_edit_preview(
    state: &crate::core::extensions::types::ToolRenderState,
    preview: EditPreview,
    args_key: Option<String>,
) -> bool {
    let current = preview_from_state(&state.get());
    let changed = match (&current, &preview) {
        (None, _) => true,
        (Some(EditPreview::Error(current)), EditPreview::Error(next)) => current != next,
        (Some(EditPreview::Diff { .. }), EditPreview::Error(_)) => true,
        (Some(EditPreview::Error(_)), EditPreview::Diff { .. }) => true,
        (
            Some(EditPreview::Diff {
                diff: current_diff,
                first_changed_line: current_line,
            }),
            EditPreview::Diff {
                diff: next_diff,
                first_changed_line: next_line,
            },
        ) => current_diff != next_diff || current_line != next_line,
    };
    state.update(|state| {
        let Value::Object(state) = state else {
            return;
        };
        state.insert(PREVIEW_KEY.to_owned(), preview_to_state(&preview));
        match &args_key {
            Some(key) => state.insert(PREVIEW_ARGS_KEY.to_owned(), Value::String(key.clone())),
            None => state.remove(PREVIEW_ARGS_KEY),
        };
        state.insert(PREVIEW_PENDING_KEY.to_owned(), Value::Bool(false));
    });
    changed
}

fn request_edit_preview(
    state: &crate::core::extensions::types::ToolRenderState,
    path: String,
    edits: Vec<Edit>,
    cwd: String,
    args_key: Option<String>,
    invalidate: Arc<dyn Fn() + Send + Sync>,
) {
    state.update(|state| {
        if let Value::Object(state) = state {
            state.insert(PREVIEW_PENDING_KEY.to_owned(), Value::Bool(true));
        }
    });
    let state = state.clone();
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
            return;
        };
        let Ok(outcome) = runtime.block_on(compute_edits_diff(&path, &edits, &cwd)) else {
            return;
        };
        let preview = match outcome {
            EditDiffOutcome::Diff(result) => EditPreview::Diff {
                diff: result.diff,
                first_changed_line: result.first_changed_line,
            },
            EditDiffOutcome::Error(error) => EditPreview::Error(error),
        };
        if preview_args_key_from_state(&state.get()) != args_key {
            return;
        }
        set_edit_preview(&state, preview, args_key);
        invalidate();
    });
}

/// `renderCall` for the edit tool.
pub fn render_call(args: &Value, theme: &Theme, context: &ToolRenderContext) -> Result<ComponentRef, HandlerError> {
    let component = get_edit_call_component(context);
    let preview_input = get_renderable_preview_input(args);
    let args_key = preview_input
        .as_ref()
        .map(|(path, _, raw_edits)| preview_args_key(path, raw_edits));

    let state_snapshot = context.state.get();
    if preview_args_key_from_state(&state_snapshot) != args_key {
        context.state.update(|state| {
            if let Value::Object(state) = state {
                state.remove(PREVIEW_KEY);
                match &args_key {
                    Some(key) => state.insert(PREVIEW_ARGS_KEY.to_owned(), Value::String(key.clone())),
                    None => state.remove(PREVIEW_ARGS_KEY),
                };
                state.insert(PREVIEW_PENDING_KEY.to_owned(), Value::Bool(false));
                state.insert(SETTLED_ERROR_KEY.to_owned(), Value::Bool(false));
            }
        });
    }

    let state_snapshot = context.state.get();
    let has_preview = state_snapshot.get(PREVIEW_KEY).is_some();
    let preview_pending = state_snapshot
        .get(PREVIEW_PENDING_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if context.args_complete
        && let Some((path, edits, _)) = preview_input
        && !has_preview
        && !preview_pending
    {
        request_edit_preview(
            &context.state,
            path,
            edits,
            context.cwd.clone(),
            args_key,
            Arc::clone(&context.invalidate),
        );
    }

    let state_snapshot = context.state.get();
    let preview = preview_from_state(&state_snapshot);
    let settled_error = state_snapshot
        .get(SETTLED_ERROR_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    fill_edit_call_component(&component, args, preview.as_ref(), settled_error, theme, &context.cwd);
    Ok(component)
}

fn format_edit_result(
    _args: &Value,
    preview: Option<&EditPreview>,
    result: &AgentToolResult,
    theme: &Theme,
    is_error: bool,
) -> Option<String> {
    let preview_diff = match preview {
        Some(EditPreview::Diff { diff, .. }) => Some(diff.as_str()),
        _ => None,
    };
    let preview_error = match preview {
        Some(EditPreview::Error(error)) => Some(error.as_str()),
        _ => None,
    };
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
        if error_text.is_empty() || Some(error_text.as_str()) == preview_error {
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
    if Some(result_diff) == preview_diff {
        return None;
    }
    Some(render_diff(result_diff))
}

/// `renderResult` for the edit tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> Result<ComponentRef, HandlerError> {
    let preview_input = get_renderable_preview_input(&context.args);
    let args_key = preview_input
        .as_ref()
        .map(|(path, _, raw_edits)| preview_args_key(path, raw_edits));
    let call_component = context.call_component.clone();
    let result_diff = (!context.is_error)
        .then(|| {
            result
                .details
                .as_ref()
                .and_then(|details| details.get("diff"))
                .and_then(Value::as_str)
        })
        .flatten();
    let mut changed = false;
    if let Some(call_component) = &call_component {
        if let Some(result_diff) = result_diff {
            let first_changed_line = result
                .details
                .as_ref()
                .and_then(|details| details.get("firstChangedLine"))
                .and_then(Value::as_u64)
                .map(|line| line as usize);
            changed = set_edit_preview(
                &context.state,
                EditPreview::Diff {
                    diff: result_diff.to_owned(),
                    first_changed_line,
                },
                args_key,
            ) || changed;
        }
        let settled_error = context
            .state
            .get()
            .get(SETTLED_ERROR_KEY)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if settled_error != context.is_error {
            context.state.update(|state| {
                if let Value::Object(state) = state {
                    state.insert(SETTLED_ERROR_KEY.to_owned(), Value::Bool(context.is_error));
                }
            });
            changed = true;
        }
        if changed {
            let preview = preview_from_state(&context.state.get());
            fill_edit_call_component(
                call_component,
                &context.args,
                preview.as_ref(),
                context.is_error,
                theme,
                &context.cwd,
            );
        }
    }

    let preview = call_component
        .is_some()
        .then(|| preview_from_state(&context.state.get()))
        .flatten();
    let mut component = Container::new();
    if let Some(output) = format_edit_result(&context.args, preview.as_ref(), result, theme, context.is_error) {
        component.add_child(Rc::new(RefCell::new(Spacer::new(1))));
        component.add_child(Rc::new(RefCell::new(Text::new(output, 1, 0))));
    }
    Ok(Rc::new(RefCell::new(component)))
}

/// `createEditToolDefinition`: the edit tool plus its renderers.
pub fn create_edit_tool_definition(cwd: &str, options: EditToolOptions) -> ToolDefinition {
    let tool: Arc<dyn AgentTool> = Arc::new(EditTool::with_options(cwd, options));
    let mut definition = create_tool_definition_from_agent_tool(tool);
    definition.prompt_snippet = Some(EDIT_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet.to_owned());
    definition.prompt_guidelines = Some(
        EDIT_TOOL_SYSTEM_PROMPT_CONTRIBUTION
            .guidelines
            .iter()
            .map(|line| (*line).to_owned())
            .collect(),
    );
    definition.render_shell = Some(RenderShell::SelfRender);
    definition.render_call = Some(Arc::new(render_call));
    definition.render_result = Some(Arc::new(render_result));
    definition
}

/// `createEditTool`: the edit tool wrapped with the shared result cap.
pub fn create_edit_tool(cwd: &str, options: EditToolOptions) -> Arc<dyn AgentTool> {
    wrap_agent_tool(Arc::new(EditTool::with_options(cwd, options)))
}
