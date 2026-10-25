//! Ported from `packages/coding-agent/src/core/tools/read.ts`.
//!
//! Text offsets/limits, oversized head-range reads, and image attachment
//! normalization, compact classifications and themed renderers.

use crate::utils::paths::PathError;
use crate::utils::paths::{node_basename, node_dirname, node_is_absolute};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{ImageContent, TextContent, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::number::js_slice_index as slice_index;
use pi_tui::Text;
use pi_tui::tui::ComponentRef;
use serde_json::Value as SchemaValue;

use super::node_fs::node_fs_error;
use super::path_utils::resolve_read_path_async;
use super::render_utils::{get_text_output, render_tool_path, replace_tabs, str_value};
use super::truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncatedBy, TruncationOptions, TruncationResult, format_size, truncate_head,
};
use crate::core::extensions::types::{HandlerError, ToolRenderContext, ToolRenderResultOptions};
use crate::modes::interactive::components::keybinding_hints::{key_hint, key_text};
use crate::modes::interactive::theme::{Theme, ThemeColor, get_language_from_path, highlight_code};
use crate::utils::image_process::{ProcessImageOptions, ProcessImageResult, process_image};
use crate::utils::mime::detect_supported_image_mime_type_from_file;
use crate::utils::paths::format_path_relative_to_cwd_or_absolute;

const DESCRIPTION: &str = "Read the contents of a file. Supports text files and images (jpg, png, gif, webp, bmp). Images are sent as attachments. For text files, output is truncated to 2000 lines or 10KB (whichever is hit first). Use offset/limit for large files. When you need the full file, continue with offset until complete.";

/// `readToolSystemPromptContribution`.
pub const READ_TOOL_SYSTEM_PROMPT_CONTRIBUTION: super::ToolSystemPromptContribution =
    super::ToolSystemPromptContribution {
        snippet: "Read file contents",
        guidelines: &["Use read to examine files instead of cat or sed."],
    };

/// Optional byte-range operation, borrowing the operation provider and path for the request.
pub type ReadRange<'a> = Box<dyn Fn(&'a JsString, f64, f64) -> BoxFuture<'a, io::Result<Vec<u8>>> + Send + Sync + 'a>;

/// File access used by [`ReadTool`]. Override to delegate reads to a remote system.
pub trait ReadOperations: Send + Sync {
    fn read_file<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<u8>>>;

    /// Optional `[start, end)` byte reader; absence selects a full-file read.
    fn read_range(&self) -> Option<ReadRange<'_>> {
        None
    }

    /// File size, or `None` when the operation does not expose `stat`.
    fn stat<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Option<f64>>> {
        let _ = absolute_path;
        Box::pin(async { Ok(None) })
    }

    fn access<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<()>>;

    /// Detects an image MIME type; `None` for non-images.
    fn detect_image_mime_type<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Option<String>>> {
        let _ = absolute_path;
        Box::pin(async { Ok(None) })
    }
}

/// Local filesystem operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalReadOperations;

impl ReadOperations for LocalReadOperations {
    fn read_file<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async move {
            super::node_fs::validate_node_path(absolute_path)?;
            tokio::fs::read(&encode_node_utf8(absolute_path))
                .await
                .map_err(|error| {
                    super::node_fs::operation_error(super::node_fs::node_promises_read_error(&error, absolute_path))
                })
        })
    }

    fn read_range(&self) -> Option<ReadRange<'_>> {
        Some(Box::new(|absolute_path, start, end| {
            let path = absolute_path.clone();
            Box::pin(async move {
                super::node_fs::validate_node_path(&path)?;
                match tokio::task::spawn_blocking(move || read_local_range(&path, start, end)).await {
                    Ok(result) => result.map_err(|error| super::node_fs::operation_error(error.message)),
                    Err(error) => std::panic::resume_unwind(error.into_panic()),
                }
            })
        }))
    }

    fn stat<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Option<f64>>> {
        Box::pin(async move {
            super::node_fs::validate_node_path(absolute_path)?;
            tokio::fs::metadata(&encode_node_utf8(absolute_path))
                .await
                .map(|metadata| Some(metadata.len() as f64))
                .map_err(|error| super::node_fs::operation_error(node_fs_error(&error, "stat", absolute_path)))
        })
    }

    fn access<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            super::node_fs::validate_node_path(absolute_path)?;
            rustix::fs::access(encode_node_utf8(absolute_path), rustix::fs::Access::READ_OK)
                .map_err(io::Error::from)
                .map_err(|error| super::node_fs::operation_error(node_fs_error(&error, "access", absolute_path)))?;
            Ok(())
        })
    }

    fn detect_image_mime_type<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Option<String>>> {
        Box::pin(async move {
            detect_supported_image_mime_type_from_file(std::path::Path::new(&encode_node_utf8(absolute_path)))
                .await
                .map_err(|error| super::node_fs::operation_error(error.message))
        })
    }
}

/// Frozen readLocalRange: one positioned read, followed by an explicit close even on failure.
fn read_local_range(
    path: &JsString,
    start: f64,
    end: f64,
) -> Result<Vec<u8>, Box<pi_js_compat::node_error::NativeNodeError>> {
    use pi_js_compat::node_error::NativeNodeError;
    use pi_js_compat::node_fs::{node_file_read_error_record, node_fs_error_record};
    use std::os::fd::IntoRawFd;
    use std::os::unix::fs::FileExt;
    let length = pi_js_compat::number::js_math_max(0.0, end - start) as usize;
    let mut buffer = vec![0_u8; length];
    let file = std::fs::File::open(encode_node_utf8(path))
        .map_err(|error| Box::new(NativeNodeError::from(node_fs_error_record(&error, "open", path))))?;
    let read = file
        .read_at(&mut buffer, start as u64)
        .map_err(|error| Box::new(NativeNodeError::from(node_file_read_error_record(&error))));
    nix::unistd::close(file.into_raw_fd())
        .map_err(io::Error::from)
        .map_err(|error| {
            Box::new(NativeNodeError::from(node_fs_error_record(
                &error,
                "close",
                &JsString::default(),
            )))
        })?;
    buffer.truncate(read?);
    Ok(buffer)
}

/// Options for [`ReadTool`].
#[derive(Clone, Default)]
pub struct ReadToolOptions {
    /// Whether to auto-resize images to 2000x2000 max. Default: true.
    pub auto_resize_images: Option<bool>,
    /// Custom file access. Default: local filesystem.
    pub operations: Option<std::sync::Arc<dyn ReadOperations>>,
}

/// Reads text files and images.
#[derive(Clone)]
pub struct ReadTool {
    cwd: JsString,
    parameters: SchemaValue,
    operations: std::sync::Arc<dyn ReadOperations>,
    auto_resize_images: bool,
    agent_config_root: Option<JsString>,
}

impl ReadTool {
    pub fn new(cwd: impl Into<JsString>) -> Self {
        Self::with_options(cwd, ReadToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<JsString>, options: ReadToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: read_schema(),
            operations: options
                .operations
                .unwrap_or_else(|| std::sync::Arc::new(LocalReadOperations)),
            auto_resize_images: options.auto_resize_images.unwrap_or(true),
            agent_config_root: None,
        }
    }
    fn execute_with_context(
        &self,
        params: JsValue,
        signal: Option<AbortSignal>,
        context: Option<crate::core::extensions::types::ExtensionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let tool = self.clone();
        Box::pin(async move { execute_read_with_abort(tool, params, signal, context).await })
    }
}

fn read_schema() -> SchemaValue {
    serde_json::json!({
        "type": "object",
        "required": ["path"],
        "properties": {
            "path": {"type": "string", "description": "Path to the file to read (relative or absolute)"},
            "offset": {"type": "number", "description": "Line number to start reading from (1-indexed)"},
            "limit": {"type": "number", "description": "Maximum number of lines to read"}
        }
    })
}

impl AgentTool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        DESCRIPTION.into()
    }

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "read".into()
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        signal: Option<AbortSignal>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        self.execute_with_context(params, signal, None)
    }
}

struct ReadInput {
    path: JsString,
    offset: Option<f64>,
    limit: Option<f64>,
}

async fn execute_read_with_abort(
    tool: ReadTool,
    params: JsValue,
    signal: Option<AbortSignal>,
    context: Option<crate::core::extensions::types::ExtensionContext>,
) -> Result<AgentToolResult, ToolError> {
    let producer_signal = signal.clone();
    super::abortable::observe_tool_producer(
        async move { execute_read(&tool, params, producer_signal, context).await },
        signal,
    )
    .await
}

async fn execute_read(
    tool: &ReadTool,
    params: JsValue,
    signal: Option<AbortSignal>,
    context: Option<crate::core::extensions::types::ExtensionContext>,
) -> Result<AgentToolResult, ToolError> {
    let operations = tool.operations.as_ref();
    let input = parse_read_input(&params)?;
    check_aborted(signal.as_ref())?;
    let absolute_path = resolve_read_tool_path(&input.path, &tool.cwd, tool.agent_config_root.as_ref()).await?;
    check_aborted(signal.as_ref())?;
    operations
        .access(&absolute_path)
        .await
        .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?;
    check_aborted(signal.as_ref())?;

    let (content, details) = read_content(tool, &input, &absolute_path, context).await?;
    check_aborted(signal.as_ref())?;
    Ok(AgentToolResult {
        content,
        details,
        is_error: None,
        terminate: None,
    })
}

async fn read_content(
    tool: &ReadTool,
    input: &ReadInput,
    absolute_path: &JsString,
    context: Option<crate::core::extensions::types::ExtensionContext>,
) -> Result<(Vec<TextOrImageContent>, Option<Value>), ToolError> {
    let operations = tool.operations.as_ref();
    let mime_type = operations
        .detect_image_mime_type(absolute_path)
        .await
        .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?;

    let model = match context {
        Some(context) => context.model().map_err(|error| ToolError::new(error.message))?,
        None => None,
    };
    let non_vision_image_note = model.is_some_and(|model| !model.input.contains(&pi_ai::types::Modality::Image));
    match mime_type.filter(|mime| !mime.is_empty()) {
        Some(mime_type) => {
            read_image(
                operations,
                absolute_path,
                &mime_type,
                tool.auto_resize_images,
                non_vision_image_note,
            )
            .await
        }
        None => read_text(operations, absolute_path, input).await,
    }
}

async fn resolve_read_tool_path(
    path: &JsString,
    cwd: &JsString,
    agent_config_root: Option<&JsString>,
) -> Result<JsString, ToolError> {
    let Some(root) = agent_config_root.filter(|root| !root.is_empty()) else {
        return resolve_read_path_async(path, cwd)
            .await
            .map_err(|error| ToolError::new(error.message().clone()));
    };
    let requested =
        super::path_utils::resolve_to_cwd(path, cwd).map_err(|error| ToolError::new(error.message().clone()))?;
    let (canonical_path, canonical_root) =
        tokio::try_join!(canonicalize_read_path(&requested), canonicalize_read_path(root))?;
    let relative = crate::utils::paths::relative_path(&canonical_root, &canonical_path)
        .map_err(|error| ToolError::new(error.message().clone()))?;
    let escapes_root = relative == ".." || relative.starts_with("../") || node_is_absolute(&relative);
    if escapes_root {
        return Err(ToolError::new(js_format!(
            "Read access denied: {} resolves outside allowed roots",
            path
        )));
    }
    Ok(canonical_path)
}

async fn canonicalize_read_path(path: &JsString) -> Result<JsString, ToolError> {
    pi_js_compat::node_fs::realpath(path)
        .await
        .map_err(|error| ToolError::new(error.message))
}

fn check_aborted(signal: Option<&AbortSignal>) -> Result<(), ToolError> {
    if signal.is_some_and(AbortSignal::aborted) {
        return Err(ToolError::new("Operation aborted"));
    }
    Ok(())
}

fn parse_read_input(params: &JsValue) -> Result<ReadInput, ToolError> {
    let path = params
        .get("path")
        .and_then(Value::as_string)
        .ok_or_else(|| ToolError::new("read tool requires a string path"))?
        .to_owned();
    Ok(ReadInput {
        path,
        offset: number_argument(params, "offset"),
        limit: number_argument(params, "limit"),
    })
}

/// Keep JS numbers intact until the operation that actually coerces an array index.
fn number_argument(params: &JsValue, name: &str) -> Option<f64> {
    params.get(name)?.as_f64()
}

async fn read_image(
    operations: &dyn ReadOperations,
    absolute_path: &JsString,
    mime_type: &str,
    auto_resize_images: bool,
    non_vision_image_note: bool,
) -> Result<(Vec<TextOrImageContent>, Option<Value>), ToolError> {
    let buffer = operations
        .read_file(absolute_path)
        .await
        .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?;
    let processed = process_image(
        &buffer,
        mime_type,
        Some(ProcessImageOptions {
            auto_resize_images: Some(auto_resize_images),
            resize_options: None,
        }),
    )
    .await;
    Ok(match processed {
        ProcessImageResult::Omitted { message } => (
            vec![TextOrImageContent::Text(TextContent::new(
                append_non_vision_image_note(
                    format!("Read image file [{mime_type}]\n{message}"),
                    non_vision_image_note,
                ),
            ))],
            None,
        ),
        ProcessImageResult::Processed { data, mime_type, hints } => {
            let mut text_note = format!("Read image file [{mime_type}]");
            if !hints.is_empty() {
                text_note.push('\n');
                text_note.push_str(&hints.join("\n"));
            }
            (
                vec![
                    TextOrImageContent::Text(TextContent::new(append_non_vision_image_note(
                        text_note,
                        non_vision_image_note,
                    ))),
                    TextOrImageContent::Image(ImageContent::new(data, mime_type)),
                ],
                None,
            )
        }
    })
}

fn append_non_vision_image_note(mut text: String, non_vision_image_note: bool) -> String {
    if non_vision_image_note {
        text.push_str("\n[Current model does not support images. The image will be omitted from this request.]");
    }
    text
}

async fn read_text(
    operations: &dyn ReadOperations,
    absolute_path: &JsString,
    input: &ReadInput,
) -> Result<(Vec<TextOrImageContent>, Option<Value>), ToolError> {
    let file_stat = operations
        .stat(absolute_path)
        .await
        .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?;
    let read_range = operations
        .read_range()
        .filter(|_| can_use_head_range_read(file_stat, input.offset, input.limit));
    let range_limited = read_range.is_some();
    let buffer = if let Some(read_range) = read_range {
        read_range(absolute_path, 0.0, DEFAULT_MAX_BYTES as f64)
            .await
            .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?
    } else {
        operations
            .read_file(absolute_path)
            .await
            .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?
    };
    let text_content = String::from_utf8_lossy(&buffer);
    let all_lines = text_content.split('\n').collect::<Vec<_>>();
    let total_file_lines = if range_limited {
        all_lines.len() + 1
    } else {
        all_lines.len()
    };

    let offset = input.offset.unwrap_or(0.0);
    let start_line = if offset == 0.0 || offset.is_nan() {
        0.0
    } else {
        pi_js_compat::number::js_math_max(0.0, offset - 1.0)
    };
    let start_line_display = start_line + 1.0;
    if start_line >= all_lines.len() as f64 {
        return Err(ToolError::new(format!(
            "Offset {} is beyond end of file ({} lines total)",
            pi_js_compat::number::js_number_to_string(offset),
            all_lines.len()
        )));
    }

    let (selected_content, user_limited_lines) = select_lines(&all_lines, start_line, input.limit);
    let truncation = truncate_head(&selected_content, TruncationOptions::default());
    let render_input = ReadRenderInput {
        path: &input.path,
        start_line_display,
        total_file_lines,
        all_lines: &all_lines,
        user_limited_lines,
        truncation: &truncation,
        range_limited,
        file_stat,
    };
    let (output_text, details) = render_read_output(&render_input)?;
    Ok((vec![TextOrImageContent::Text(TextContent::new(output_text))], details))
}

fn can_use_head_range_read(file_stat: Option<f64>, offset: Option<f64>, limit: Option<f64>) -> bool {
    let Some(size) = file_stat else {
        return false;
    };
    if size <= DEFAULT_MAX_BYTES as f64 {
        return false;
    }
    if offset.is_some_and(|offset| offset > 1.0) {
        return false;
    }
    limit.is_none_or(|limit| limit > DEFAULT_MAX_LINES as f64)
}

fn select_lines(all_lines: &[&str], start_line: f64, limit: Option<f64>) -> (String, Option<f64>) {
    let start_index = slice_index(start_line, all_lines.len());
    let (end_index, limited_lines) = match limit {
        Some(limit) => {
            let end_line = pi_js_compat::number::js_math_min(start_line + limit, all_lines.len() as f64);
            (slice_index(end_line, all_lines.len()), Some(end_line - start_line))
        }
        None => (all_lines.len(), None),
    };
    let end_index = end_index.max(start_index);
    (all_lines[start_index..end_index].join("\n"), limited_lines)
}

struct ReadRenderInput<'a> {
    path: &'a JsString,
    start_line_display: f64,
    total_file_lines: usize,
    all_lines: &'a [&'a str],
    user_limited_lines: Option<f64>,
    truncation: &'a TruncationResult,
    range_limited: bool,
    file_stat: Option<f64>,
}

fn render_read_output(input: &ReadRenderInput<'_>) -> Result<(JsString, Option<Value>), ToolError> {
    let ReadRenderInput {
        path,
        start_line_display,
        total_file_lines,
        all_lines,
        user_limited_lines,
        truncation,
        range_limited,
        file_stat,
    } = *input;
    let start_label = pi_js_compat::number::js_number_to_string(start_line_display);
    let end_line_display = start_line_display + truncation.output_lines as f64 - 1.0;
    let end_label = pi_js_compat::number::js_number_to_string(end_line_display);
    let next_label = pi_js_compat::number::js_number_to_string(end_line_display + 1.0);
    if range_limited && !truncation.truncated {
        let output_text = truncation.content.clone()
            + format!(
                "\n\n[Showing lines {start_label}-{end_label} of file ({} limit). Use offset={next_label} to continue.]",
                format_size(DEFAULT_MAX_BYTES as f64),
            );
        return Ok((output_text, Some(truncation_details(truncation, true, file_stat))));
    }
    if truncation.first_line_exceeds_limit {
        let index = start_line_display - 1.0;
        if index.fract() != 0.0 {
            // Frozen reads allLines[startLine], not slice(startLine), at this boundary.
            return Err(ToolError::new(
                "The \"string\" argument must be of type string or an instance of Buffer or ArrayBuffer. Received undefined",
            ));
        }
        let first_line_size = format_size(all_lines[index as usize].len() as f64);
        let output_text = JsString::from(format!(
            "[Line {start_label} is {first_line_size}, exceeds {} limit. Use bash: sed -n '{start_label}p' ",
            format_size(DEFAULT_MAX_BYTES as f64),
        )) + path
            + format!(" | head -c {}]", DEFAULT_MAX_BYTES);
        return Ok((output_text, Some(truncation_details(truncation, false, None))));
    }
    if truncation.truncated {
        let mut output_text = truncation.content.clone();
        if truncation.truncated_by == Some(TruncatedBy::Lines) {
            output_text.push_str(format!(
                "\n\n[Showing lines {start_label}-{end_label} of {total_file_lines}. Use offset={next_label} to continue.]",
            ));
        } else {
            output_text.push_str(format!(
                "\n\n[Showing lines {start_label}-{end_label} of {total_file_lines} ({} limit). Use offset={next_label} to continue.]",
                format_size(DEFAULT_MAX_BYTES as f64),
            ));
        }
        return Ok((output_text, Some(truncation_details(truncation, false, None))));
    }
    if let Some(user_limited_lines) = user_limited_lines {
        let consumed = start_line_display - 1.0 + user_limited_lines;
        if consumed < all_lines.len() as f64 {
            let remaining = pi_js_compat::number::js_number_to_string(all_lines.len() as f64 - consumed);
            let next = pi_js_compat::number::js_number_to_string(consumed + 1.0);
            return Ok((
                truncation.content.clone()
                    + format!("\n\n[{remaining} more lines in file. Use offset={next} to continue.]",),
                None,
            ));
        }
    }
    Ok((truncation.content.clone(), None))
}

fn truncation_details(truncation: &TruncationResult, force_bytes: bool, total_bytes: Option<f64>) -> Value {
    let mut value = pi_js_compat::js_json::to_value(truncation).expect("truncation metadata must serialize");
    if let Value::Object(details) = &mut value
        && force_bytes
    {
        details.insert("truncated".into(), Value::Bool(true));
        details.insert("truncatedBy".into(), json!("bytes"));
        if let Some(total_bytes) = total_bytes {
            details.insert("totalBytes".into(), Value::Number(total_bytes));
        }
    }
    Value::Object([("truncation".into(), value)].into_iter().collect())
}

// ============================================================================
// Renderers
// ============================================================================

fn render_args_path(args: &JsValue) -> Option<JsString> {
    let value = args.get("file_path").filter(|value| !value.is_null());
    let value = value.or_else(|| args.get("path"));
    str_value(value)
}

fn format_read_line_range(args: &JsValue, theme: &Theme) -> JsString {
    let offset = args.get("offset").and_then(Value::as_f64);
    let limit = args.get("limit").and_then(Value::as_f64);
    if offset.is_none() && limit.is_none() {
        return JsString::default();
    }
    let start_line = offset.unwrap_or(1.0);
    let end_line = limit.map(|limit| start_line + limit - 1.0);
    let start = pi_js_compat::number::js_number_to_string(start_line);
    match end_line {
        Some(end_line) if end_line != 0.0 && !end_line.is_nan() => theme.fg(
            ThemeColor::Warning,
            format!(":{start}-{}", pi_js_compat::number::js_number_to_string(end_line)),
        ),
        _ => theme.fg(ThemeColor::Warning, format!(":{start}")),
    }
}

fn format_read_call(args: &JsValue, theme: &Theme, cwd: &JsString) -> Result<JsString, PathError> {
    let path_display = render_tool_path(render_args_path(args).as_ref(), theme, cwd, None)?;
    Ok(js_format!(
        "{} {}{}",
        theme.fg(ThemeColor::ToolTitle, theme.bold("read")),
        path_display,
        format_read_line_range(args, theme)
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CompactReadKind {
    Docs,
    Resource,
    Skill,
}

struct CompactReadClassification {
    kind: CompactReadKind,
    label: JsString,
}

const COMPACT_RESOURCE_FILE_NAMES: [&str; 8] = [
    "AGENTS.md",
    "AGENTS.local.md",
    "AGENTS.MD",
    "AGENTS.local.MD",
    "CLAUDE.md",
    "CLAUDE.local.md",
    "CLAUDE.MD",
    "CLAUDE.local.MD",
];

fn get_pi_docs_classification(absolute_path: &JsString) -> Result<Option<CompactReadClassification>, HandlerError> {
    let readme_path = crate::config::get_readme_path().map_err(HandlerError::from)?;
    let package_root = crate::utils::paths::node_dirname(&readme_path);
    let relative_path = crate::utils::paths::relative_path(&package_root, absolute_path).map_err(HandlerError::from)?;
    if relative_path.is_empty()
        || relative_path == ".."
        || relative_path.starts_with("../")
        || node_is_absolute(&relative_path)
    {
        return Ok(None);
    }
    // POSIX sep is '/', so frozen toPosixPath leaves literal backslashes intact.
    let label = relative_path;
    if label == "README.md" || label.starts_with("docs/") || label.starts_with("examples/") {
        return Ok(Some(CompactReadClassification {
            kind: CompactReadKind::Docs,
            label,
        }));
    }
    Ok(None)
}

fn get_compact_read_classification(
    args: &JsValue,
    cwd: &JsString,
) -> Result<Option<CompactReadClassification>, HandlerError> {
    let Some(raw_path) = render_args_path(args).filter(|path| !path.is_empty()) else {
        return Ok(None);
    };
    let absolute_path = super::path_utils::resolve_to_cwd(&raw_path, cwd).map_err(HandlerError::from)?;
    let file_name = node_basename(&absolute_path);
    if file_name == "SKILL.md" {
        let directory_name = node_basename(node_dirname(&absolute_path));
        let label = if directory_name.is_empty() {
            file_name
        } else {
            directory_name
        };
        return Ok(Some(CompactReadClassification {
            kind: CompactReadKind::Skill,
            label,
        }));
    }
    if let Some(classification) = get_pi_docs_classification(&absolute_path)? {
        return Ok(Some(classification));
    }
    if COMPACT_RESOURCE_FILE_NAMES.iter().any(|name| file_name == *name) {
        let label = format_path_relative_to_cwd_or_absolute(&absolute_path, cwd).map_err(HandlerError::from)?;
        return Ok(Some(CompactReadClassification {
            kind: CompactReadKind::Resource,
            label,
        }));
    }
    Ok(None)
}

fn format_compact_read_call(classification: &CompactReadClassification, args: &JsValue, theme: &Theme) -> JsString {
    let expand_hint = theme.fg(
        ThemeColor::Dim,
        js_format!(" ({} to expand)", key_text("app.tools.expand")),
    );
    if classification.kind == CompactReadKind::Skill {
        return js_format!(
            "{}{}{}{}",
            theme.fg(ThemeColor::CustomMessageLabel, "\u{1b}[1m[skill]\u{1b}[22m "),
            theme.fg(ThemeColor::CustomMessageText, &classification.label),
            format_read_line_range(args, theme),
            expand_hint
        );
    }
    let kind = match classification.kind {
        CompactReadKind::Docs => "docs",
        CompactReadKind::Resource => "resource",
        CompactReadKind::Skill => "skill",
    };
    js_format!(
        "{} {}{}{}",
        theme.fg(ThemeColor::ToolTitle, theme.bold(format!("read {kind}"))),
        theme.fg(ThemeColor::Accent, &classification.label),
        format_read_line_range(args, theme),
        expand_hint
    )
}

fn trim_trailing_empty_lines(mut lines: Vec<JsString>) -> Vec<JsString> {
    let mut end = lines.len();
    while end > 0 && lines[end - 1].is_empty() {
        end -= 1;
    }
    lines.truncate(end);
    lines
}

fn format_read_result(
    args: &JsValue,
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    show_images: bool,
    is_error: bool,
) -> Result<JsString, HandlerError> {
    if !options.expanded && !is_error {
        return Ok(JsString::default());
    }
    let raw_path = render_args_path(args);
    let output = get_text_output(Some(result), show_images);
    let lang = raw_path.as_ref().and_then(get_language_from_path);
    let rendered_lines = match lang.as_ref() {
        Some(lang) => highlight_code(&replace_tabs(&output), Some(lang))?,
        None => output.split('\n').collect(),
    };
    let lines = trim_trailing_empty_lines(rendered_lines);
    let max_lines = if options.expanded { lines.len() } else { 10 };
    let display_lines = lines.iter().take(max_lines).collect::<Vec<_>>();
    let remaining = lines.len().saturating_sub(max_lines);
    let mut text = js_format!(
        "\n{}",
        JsString::join(
            display_lines.iter().map(|line| match lang {
                Some(_) => replace_tabs(line),
                None => theme.fg(ThemeColor::ToolOutput, replace_tabs(line)),
            }),
            "\n"
        )
    );
    if remaining > 0 {
        text.push_str(js_format!(
            "{}{}{}",
            theme.fg(ThemeColor::Muted, format!("\n... ({remaining} more lines,")) + " ",
            key_hint("app.tools.expand", "to expand"),
            theme.fg(ThemeColor::Muted, ")")
        ));
    }
    if let Some(truncation) = result.details.as_ref().and_then(|details| details.get("truncation"))
        && truncation.get("truncated").is_some_and(json::is_truthy)
    {
        text.push_str(js_format!(
            "\n{}",
            theme.fg(ThemeColor::Warning, format_read_truncation(truncation))
        ));
    }
    Ok(text)
}

fn format_read_truncation(truncation: &JsValue) -> JsString {
    let max_bytes = truncation
        .get("maxBytes")
        .and_then(Value::as_f64)
        .unwrap_or(DEFAULT_MAX_BYTES as f64);
    if truncation.get("firstLineExceedsLimit").is_some_and(json::is_truthy) {
        return js_format!("[First line exceeds {} limit]", format_size(max_bytes));
    }
    let output_lines = truncation_field_text(truncation, "outputLines");
    if truncation
        .get("truncatedBy")
        .and_then(Value::as_string)
        .is_some_and(|value| value == "lines")
    {
        let total_lines = truncation_field_text(truncation, "totalLines");
        let max_lines = truncation
            .get("maxLines")
            .and_then(Value::as_f64)
            .unwrap_or(DEFAULT_MAX_LINES as f64);
        return js_format!(
            "[Truncated: showing {} of {} lines ({} line limit)]",
            output_lines,
            total_lines,
            pi_js_compat::number::js_number_to_string(max_lines)
        );
    }
    js_format!(
        "[Truncated: {} lines shown ({} limit)]",
        output_lines,
        format_size(max_bytes)
    )
}

fn truncation_field_text(truncation: &JsValue, field: &str) -> JsString {
    truncation
        .get(field)
        .map(|value| json::to_js_string(value).expect("read truncation numeric fields are string-coercible"))
        .unwrap_or_else(|| "undefined".into())
}

/// `renderCall` for the read tool.
pub fn render_call(args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> Result<ComponentRef, HandlerError> {
    let classification = if context.expanded {
        None
    } else {
        get_compact_read_classification(args, &context.cwd)?
    };
    let text = match classification {
        Some(classification) => Ok(format_compact_read_call(&classification, args, theme)),
        None => format_read_call(args, theme, &context.cwd),
    }
    .map_err(HandlerError::from)?;
    Ok(Rc::new(RefCell::new(Text::new(text, 0, 0))))
}

/// `renderResult` for the read tool.
pub fn render_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> Result<ComponentRef, HandlerError> {
    let text = format_read_result(
        &context.args,
        result,
        options,
        theme,
        context.show_images,
        context.is_error,
    )?;
    Ok(Rc::new(RefCell::new(Text::new(text, 0, 0))))
}

/// `createReadToolDefinition`: the read tool plus its renderers.
pub fn create_read_tool_definition(
    cwd: impl Into<JsString>,
    options: ReadToolOptions,
) -> crate::core::extensions::types::ToolDefinition {
    definition_from_read_tool(ReadTool::with_options(cwd, options))
}

/// `createAgentConfigReadToolDefinition`: reads canonical paths within the supplied root.
pub fn create_agent_config_read_tool_definition(
    cwd: impl Into<JsString>,
    agent_config_root: impl Into<JsString>,
) -> crate::core::extensions::types::ToolDefinition {
    let mut tool = ReadTool::new(cwd);
    tool.agent_config_root = Some(agent_config_root.into());
    definition_from_read_tool(tool)
}

fn definition_from_read_tool(tool: ReadTool) -> crate::core::extensions::types::ToolDefinition {
    let tool = std::sync::Arc::new(tool);
    let mut definition = super::tool_definition_wrapper::create_tool_definition_from_agent_tool(tool.clone());
    definition.execute = std::sync::Arc::new(move |_, params, signal, _, context| {
        let result = tool.execute_with_context(params, signal, context);
        Box::pin(async move { result.await.map_err(|error| HandlerError::new(error.message)) })
    });
    definition.prompt_snippet = Some(READ_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet.into());
    definition.prompt_guidelines = Some(
        READ_TOOL_SYSTEM_PROMPT_CONTRIBUTION
            .guidelines
            .iter()
            .map(|line| (*line).into())
            .collect(),
    );
    definition.approval_required = Some(false);
    definition.approval_kind = Some(crate::core::extensions::types::ApprovalKind::ReadOnly);
    definition.render_call = Some(std::sync::Arc::new(render_call));
    definition.render_result = Some(std::sync::Arc::new(render_result));
    definition
}

/// `createReadTool`: the read tool wrapped with the shared result cap.
pub fn create_read_tool(cwd: impl Into<JsString>, options: ReadToolOptions) -> std::sync::Arc<dyn AgentTool> {
    super::tool_definition_wrapper::wrap_agent_tool(std::sync::Arc::new(ReadTool::with_options(cwd, options)))
}
