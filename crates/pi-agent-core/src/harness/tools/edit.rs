use std::sync::Arc;

use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::{Map, Value, json};

use crate::agent::{AgentTool, AgentToolResult, ToolError};
use crate::harness::tools::edit_diff::{
    Edit, apply_edits_to_normalized_content, detect_line_ending, generate_diff_string, generate_unified_patch,
    normalize_to_lf, restore_line_endings, strip_bom,
};
use crate::harness::tools::path_utils::resolve_tool_path;
use crate::harness::tools::tool_context::ExecutionToolContext;
use crate::harness::types::{ExecutionEnv, FileError, FileKind};
use tokio_util::sync::CancellationToken;

const DESCRIPTION: &str = "Edit a single file using exact text replacement. Every edits[].oldText must match a unique, non-overlapping region of the original file. If two changes affect the same block or nearby lines, merge them into one edit instead of emitting overlapping edits. Do not include large unchanged regions just to connect distant changes.";

/// Applies exact-text replacements to one file.
pub struct EditTool {
    context: Arc<ExecutionToolContext>,
    parameters: Value,
}

impl EditTool {
    pub fn new(context: Arc<ExecutionToolContext>) -> Self {
        Self {
            context,
            parameters: edit_schema(),
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
                "items": {
                    "type": "object",
                    "required": ["oldText", "newText"],
                    "properties": {
                        "oldText": {"type": "string", "description": "Exact text for one targeted replacement. It must be unique in the original file and must not overlap with any other edits[].oldText in the same call."},
                        "newText": {"type": "string", "description": "Replacement text for this targeted edit."}
                    }
                },
                "description": "One or more targeted replacements. Each edit is matched against the original file, not incrementally. Do not include overlapping or nested edits. If two changes touch the same block or nearby lines, merge them into one edit instead."
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
        _on_update: crate::agent::AgentToolUpdateCallback,
        _execution: Option<crate::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let context = Arc::clone(&self.context);
        Box::pin(async move { execute_edit(context, params, signal).await })
    }
}

fn prepare_edit_arguments(input: Value) -> Value {
    let Value::Object(mut args) = input else {
        return input;
    };
    parse_serialized_edits(&mut args);
    merge_legacy_edits(args)
}

fn parse_serialized_edits(args: &mut Map<String, Value>) {
    if let Some(Value::String(serialized)) = args.get("edits")
        && let Ok(Value::Array(parsed)) = serde_json::from_str::<Value>(serialized)
    {
        args.insert("edits".to_owned(), Value::Array(parsed));
    }
}

fn merge_legacy_edits(args: Map<String, Value>) -> Value {
    let legacy_old_text = args.get("oldText").and_then(Value::as_str).map(str::to_owned);
    let legacy_new_text = args.get("newText").and_then(Value::as_str).map(str::to_owned);
    let (Some(old_text), Some(new_text)) = (legacy_old_text, legacy_new_text) else {
        return Value::Object(args);
    };

    let mut edits = match args.get("edits") {
        Some(Value::Array(edits)) => edits.clone(),
        _ => Vec::new(),
    };
    edits.push(json!({"oldText": old_text, "newText": new_text}));
    let mut result = Map::new();
    for (key, value) in args {
        match key.as_str() {
            "oldText" | "newText" => {}
            "edits" => {
                result.insert(key, Value::Array(edits.clone()));
            }
            _ => {
                result.insert(key, value);
            }
        }
    }
    if !result.contains_key("edits") {
        result.insert("edits".to_owned(), Value::Array(edits));
    }
    Value::Object(result)
}

fn parse_edit_input(input: &Value) -> Result<(String, Vec<Edit>), ToolError> {
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("edit tool requires a string path"))?
        .to_owned();
    let edits = parse_edit_list(input.get("edits"))?;
    Ok((path, edits))
}

fn parse_edit_list(value: Option<&Value>) -> Result<Vec<Edit>, ToolError> {
    let Some(Value::Array(edits)) = value else {
        return Err(ToolError::new(
            "Edit tool input is invalid. edits must contain at least one replacement.",
        ));
    };
    if edits.is_empty() {
        return Err(ToolError::new(
            "Edit tool input is invalid. edits must contain at least one replacement.",
        ));
    }
    edits
        .iter()
        .map(|edit| {
            Ok(Edit {
                old_text: edit
                    .get("oldText")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::new("edit tool requires string oldText"))?
                    .to_owned(),
                new_text: edit
                    .get("newText")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::new("edit tool requires string newText"))?
                    .to_owned(),
            })
        })
        .collect()
}

fn edit_access_error(path: &str, error: &FileError) -> ToolError {
    ToolError::new(format!(
        "Could not edit file: {path}. Error code: {}.",
        error.code.as_str()
    ))
}

async fn execute_edit(
    context: Arc<ExecutionToolContext>,
    params: Value,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let (path, edits) = parse_edit_input(&params)?;
    let absolute_path = resolve_tool_path(context.env.as_ref(), &path, signal.as_ref())
        .await
        .map_err(|error| ToolError::new(error.message))?;
    let env = Arc::clone(&context.env);
    let operation = apply_edit_operation(env.as_ref(), &path, &edits, &absolute_path, signal.as_ref());
    context
        .file_mutation_queue
        .run(env.as_ref(), &absolute_path, operation)
        .await
        .map_err(|error| ToolError::new(error.message))?
}

async fn apply_edit_operation(
    env: &dyn ExecutionEnv,
    path: &str,
    edits: &[Edit],
    absolute_path: &str,
    signal: Option<&CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    ensure_not_aborted(signal)?;
    let content = read_editable_file(env, path, absolute_path, signal).await?;
    let applied = apply_edits_to_normalized_content(&content.normalized, edits, path)
        .map_err(|error| ToolError::new(error.message))?;
    ensure_not_aborted(signal)?;

    let final_content = format!(
        "{}{}",
        content.bom,
        restore_line_endings(&applied.new_content, content.line_ending)
    );
    env.write_file(
        absolute_path,
        crate::harness::types::FileContent::Text(&final_content),
        signal,
    )
    .await
    .map_err(|error| edit_access_error(path, &error))?;
    ensure_not_aborted(signal)?;
    Ok(edit_success_result(path, edits.len(), &applied))
}

struct EditableContent {
    bom: String,
    line_ending: crate::harness::tools::edit_diff::LineEnding,
    normalized: String,
}

async fn read_editable_file(
    env: &dyn ExecutionEnv,
    path: &str,
    absolute_path: &str,
    signal: Option<&CancellationToken>,
) -> Result<EditableContent, ToolError> {
    let info = env
        .file_info(absolute_path)
        .await
        .map_err(|error| edit_access_error(path, &error))?;
    if !matches!(info.kind, FileKind::File | FileKind::Symlink) {
        return Err(ToolError::new(format!(
            "Could not edit file: {path}. Path is not a file."
        )));
    }
    let text = env
        .read_text_file(absolute_path, signal)
        .await
        .map_err(|error| edit_access_error(path, &error))?;
    ensure_not_aborted(signal)?;
    let stripped = strip_bom(&text);
    Ok(EditableContent {
        line_ending: detect_line_ending(&stripped.text),
        normalized: normalize_to_lf(&stripped.text),
        bom: stripped.bom,
    })
}

fn ensure_not_aborted(signal: Option<&CancellationToken>) -> Result<(), ToolError> {
    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Err(ToolError::new("Operation aborted"));
    }
    Ok(())
}

fn edit_success_result(
    path: &str,
    edit_count: usize,
    applied: &crate::harness::tools::edit_diff::AppliedEditsResult,
) -> AgentToolResult {
    let diff_result = generate_diff_string(&applied.base_content, &applied.new_content, 4);
    let mut details = Map::new();
    details.insert("diff".to_owned(), Value::String(diff_result.diff));
    details.insert(
        "patch".to_owned(),
        Value::String(generate_unified_patch(
            path,
            &applied.base_content,
            &applied.new_content,
            4,
        )),
    );
    if let Some(first_changed_line) = diff_result.first_changed_line {
        details.insert("firstChangedLine".to_owned(), json!(first_changed_line));
    }
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(format!(
            "Successfully replaced {edit_count} block(s) in {path}."
        )))],
        details: Some(Value::Object(details)),
        usage: None,
        added_tool_names: None,
        is_error: None,
        terminate: None,
    }
}
