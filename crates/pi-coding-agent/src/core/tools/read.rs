//! Ported from `packages/coding-agent/src/core/tools/read.ts`.
//!
//! Text offsets/limits, oversized head-range reads, and image attachment
//! normalization. Renderers are pending `theme.ts`.

use crate::utils::paths::{node_basename, node_dirname, node_is_absolute};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{ImageContent, TextContent, TextOrImageContent};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;
use pi_tui::Text;
use pi_tui::tui::ComponentRef;
use serde_json::Value as SchemaValue;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::sync::CancellationToken;

use super::node_fs::node_fs_error;
use super::path_utils::resolve_read_path_async;
use super::render_utils::{get_text_output, render_tool_path, replace_tabs, str_value};
use super::truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncatedBy, TruncationOptions, TruncationResult, format_size, truncate_head,
};
use crate::core::extensions::types::{
    ToolRenderContext, ToolRenderResultOptions, infallible_render_call, infallible_render_result,
};
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

/// File access used by [`ReadTool`]. Override to delegate reads to a remote system.
pub trait ReadOperations: Send + Sync {
    fn read_file<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<u8>>>;

    /// Whether [`ReadOperations::read_range`] is implemented.
    fn supports_read_range(&self) -> bool {
        false
    }

    /// Reads `[start, end)` bytes. Only called when [`ReadOperations::supports_read_range`] is true.
    fn read_range<'a>(
        &'a self,
        absolute_path: &'a JsString,
        start: u64,
        end: u64,
    ) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        let _ = (absolute_path, start, end);
        Box::pin(async { Err(io::Error::other("readRange is not implemented")) })
    }

    /// File size, or `None` when the operation does not expose `stat`.
    fn stat<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Option<u64>>> {
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
            tokio::fs::read(&encode_node_utf8(absolute_path))
                .await
                .map_err(|error| super::node_fs::operation_error(node_fs_error(&error, "open", &absolute_path)))
        })
    }

    fn supports_read_range(&self) -> bool {
        true
    }

    fn read_range<'a>(
        &'a self,
        absolute_path: &'a JsString,
        start: u64,
        end: u64,
    ) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async move {
            let length = end.saturating_sub(start);
            let mut file = tokio::fs::File::open(&encode_node_utf8(absolute_path))
                .await
                .map_err(|error| super::node_fs::operation_error(node_fs_error(&error, "read", &absolute_path)))?;
            file.seek(io::SeekFrom::Start(start))
                .await
                .map_err(|error| super::node_fs::operation_error(node_fs_error(&error, "read", &absolute_path)))?;
            let mut buffer = vec![0; length as usize];
            let mut bytes_read = 0;
            while bytes_read < buffer.len() {
                let read = file
                    .read(&mut buffer[bytes_read..])
                    .await
                    .map_err(|error| super::node_fs::operation_error(node_fs_error(&error, "read", &absolute_path)))?;
                if read == 0 {
                    break;
                }
                bytes_read += read;
            }
            buffer.truncate(bytes_read);
            Ok(buffer)
        })
    }

    fn stat<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Option<u64>>> {
        Box::pin(async move {
            tokio::fs::metadata(&encode_node_utf8(absolute_path))
                .await
                .map(|metadata| Some(metadata.len()))
                .map_err(|error| super::node_fs::operation_error(node_fs_error(&error, "stat", &absolute_path)))
        })
    }

    fn access<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            tokio::fs::metadata(&encode_node_utf8(absolute_path))
                .await
                .map_err(|error| super::node_fs::operation_error(node_fs_error(&error, "access", &absolute_path)))?;
            Ok(())
        })
    }

    fn detect_image_mime_type<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, io::Result<Option<String>>> {
        Box::pin(async move {
            detect_supported_image_mime_type_from_file(std::path::Path::new(&encode_node_utf8(absolute_path)))
                .map(|mime_type| mime_type.map(|value| value.to_owned()))
                .map_err(|error| super::node_fs::operation_error(node_fs_error(&error, "open", &absolute_path)))
        })
    }
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
pub struct ReadTool {
    cwd: JsString,
    parameters: SchemaValue,
    operations: std::sync::Arc<dyn ReadOperations>,
    auto_resize_images: bool,
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
        }
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

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> &str {
        "read"
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
        let operations = std::sync::Arc::clone(&self.operations);
        let auto_resize_images = self.auto_resize_images;
        Box::pin(async move { execute_read(&cwd, operations, auto_resize_images, params, signal).await })
    }
}

struct ReadInput {
    path: JsString,
    offset: Option<usize>,
    limit: Option<usize>,
}

async fn execute_read(
    cwd: &JsString,
    operations: std::sync::Arc<dyn ReadOperations>,
    auto_resize_images: bool,
    params: JsValue,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let input = parse_read_input(&params)?;
    check_aborted(signal.as_ref())?;
    let absolute_path = resolve_read_path_async(&input.path, cwd)
        .await
        .map_err(|error| ToolError::new(error.message().clone()))?;
    operations
        .access(&absolute_path)
        .await
        .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?;
    check_aborted(signal.as_ref())?;

    let mime_type = operations
        .detect_image_mime_type(&absolute_path)
        .await
        .map_err(|error| ToolError::new(super::node_fs::operation_error_message(&error)))?;

    let (content, details) = match mime_type {
        Some(mime_type) => read_image(operations.as_ref(), &absolute_path, &mime_type, auto_resize_images).await?,
        None => read_text(operations.as_ref(), &absolute_path, &input).await?,
    };
    check_aborted(signal.as_ref())?;
    Ok(AgentToolResult {
        content,
        details,
        is_error: None,
        terminate: None,
    })
}

fn check_aborted(signal: Option<&CancellationToken>) -> Result<(), ToolError> {
    if signal.is_some_and(CancellationToken::is_cancelled) {
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

/// JavaScript `Number` to array-index conversion: truncate toward zero, non-finite as 0.
fn number_argument(params: &JsValue, name: &str) -> Option<usize> {
    let value = params.get(name)?.as_f64()?;
    if !value.is_finite() || value <= 0.0 {
        return Some(0);
    }
    Some(value.trunc() as usize)
}

async fn read_image(
    operations: &dyn ReadOperations,
    absolute_path: &JsString,
    mime_type: &str,
    auto_resize_images: bool,
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
            vec![TextOrImageContent::Text(TextContent::new(format!(
                "Read image file [{mime_type}]\n{message}"
            )))],
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
                    TextOrImageContent::Text(TextContent::new(text_note)),
                    TextOrImageContent::Image(ImageContent::new(data, mime_type)),
                ],
                None,
            )
        }
    })
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
    let range_limited = can_use_head_range_read(file_stat, operations, input.offset, input.limit);
    let buffer = if range_limited {
        operations
            .read_range(absolute_path, 0, DEFAULT_MAX_BYTES as u64)
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

    let start_line = input.offset.unwrap_or(0).saturating_sub(1);
    let start_line_display = start_line + 1;
    if start_line >= all_lines.len() {
        return Err(ToolError::new(format!(
            "Offset {} is beyond end of file ({} lines total)",
            input.offset.unwrap_or(0),
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
    let (output_text, details) = render_read_output(&render_input);
    Ok((vec![TextOrImageContent::Text(TextContent::new(output_text))], details))
}

fn can_use_head_range_read(
    file_stat: Option<u64>,
    operations: &dyn ReadOperations,
    offset: Option<usize>,
    limit: Option<usize>,
) -> bool {
    let Some(size) = file_stat else {
        return false;
    };
    if !operations.supports_read_range() || size <= DEFAULT_MAX_BYTES as u64 {
        return false;
    }
    if offset.is_some_and(|offset| offset > 1) {
        return false;
    }
    limit.is_none_or(|limit| limit > DEFAULT_MAX_LINES)
}

fn select_lines(all_lines: &[&str], start_line: usize, limit: Option<usize>) -> (String, Option<usize>) {
    match limit {
        Some(limit) => {
            let end_line = (start_line + limit).min(all_lines.len());
            (all_lines[start_line..end_line].join("\n"), Some(end_line - start_line))
        }
        None => (all_lines[start_line..].join("\n"), None),
    }
}

struct ReadRenderInput<'a> {
    path: &'a JsString,
    start_line_display: usize,
    total_file_lines: usize,
    all_lines: &'a [&'a str],
    user_limited_lines: Option<usize>,
    truncation: &'a TruncationResult,
    range_limited: bool,
    file_stat: Option<u64>,
}

fn render_read_output(input: &ReadRenderInput<'_>) -> (JsString, Option<Value>) {
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
    if range_limited && !truncation.truncated {
        let end_line_display = start_line_display + truncation.output_lines - 1;
        let next_offset = end_line_display + 1;
        let output_text = truncation.content.clone()
            + format!(
                "\n\n[Showing lines {}-{} of file ({} limit). Use offset={} to continue.]",
                start_line_display,
                end_line_display,
                format_size(DEFAULT_MAX_BYTES),
                next_offset
            );
        return (output_text, Some(truncation_details(truncation, true, file_stat)));
    }
    if truncation.first_line_exceeds_limit {
        let first_line_size = format_size(all_lines[start_line_display - 1].len());
        let output_text = JsString::from(format!(
            "[Line {} is {}, exceeds {} limit. Use bash: sed -n '{}p' ",
            start_line_display,
            first_line_size,
            format_size(DEFAULT_MAX_BYTES),
            start_line_display,
        )) + path
            + format!(" | head -c {}]", DEFAULT_MAX_BYTES);
        return (output_text, Some(truncation_details(truncation, false, None)));
    }
    if truncation.truncated {
        let end_line_display = start_line_display + truncation.output_lines - 1;
        let next_offset = end_line_display + 1;
        let mut output_text = truncation.content.clone();
        if truncation.truncated_by == Some(TruncatedBy::Lines) {
            output_text.push_str(&format!(
                "\n\n[Showing lines {}-{} of {}. Use offset={} to continue.]",
                start_line_display, end_line_display, total_file_lines, next_offset
            ));
        } else {
            output_text.push_str(&format!(
                "\n\n[Showing lines {}-{} of {} ({} limit). Use offset={} to continue.]",
                start_line_display,
                end_line_display,
                total_file_lines,
                format_size(DEFAULT_MAX_BYTES),
                next_offset
            ));
        }
        return (output_text, Some(truncation_details(truncation, false, None)));
    }
    if let Some(user_limited_lines) = user_limited_lines {
        let consumed = start_line_display - 1 + user_limited_lines;
        if consumed < all_lines.len() {
            let remaining = all_lines.len() - consumed;
            let next_offset = consumed + 1;
            return (
                truncation.content.clone()
                    + format!(
                        "\n\n[{} more lines in file. Use offset={} to continue.]",
                        remaining, next_offset
                    ),
                None,
            );
        }
    }
    (truncation.content.clone(), None)
}

fn truncation_details(truncation: &TruncationResult, force_bytes: bool, total_bytes: Option<u64>) -> Value {
    let mut value = pi_js_compat::js_json::to_value(truncation).expect("truncation metadata must serialize");
    if let Value::Object(details) = &mut value
        && force_bytes
    {
        details.insert("truncated".into(), Value::Bool(true));
        details.insert("truncatedBy".into(), json!("bytes"));
        if let Some(total_bytes) = total_bytes {
            details.insert("totalBytes".into(), json!(total_bytes));
        }
    }
    json!({"truncation": value})
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
        Some(end_line) => theme.fg(
            ThemeColor::Warning,
            &format!(":{start}-{}", pi_js_compat::number::js_number_to_string(end_line)),
        ),
        None => theme.fg(ThemeColor::Warning, &format!(":{start}")),
    }
}

fn format_read_call(args: &JsValue, theme: &Theme, cwd: &JsString) -> JsString {
    let path_display = render_tool_path(render_args_path(args).as_ref(), theme, cwd, None);
    js_format!(
        "{} {}{}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("read")),
        path_display,
        format_read_line_range(args, theme)
    )
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

const COMPACT_RESOURCE_FILE_NAMES: [&str; 9] = [
    "AGENTS.override.md",
    "AGENTS.md",
    "AGENTS.local.md",
    "AGENTS.MD",
    "AGENTS.local.MD",
    "CLAUDE.md",
    "CLAUDE.local.md",
    "CLAUDE.MD",
    "CLAUDE.local.MD",
];

fn get_pi_docs_classification(absolute_path: &JsString) -> Option<CompactReadClassification> {
    let package_root = crate::config::get_readme_path().parent()?.to_path_buf();
    let package_root = package_root.to_string_lossy().into_owned();
    let relative_path = crate::utils::paths::relative_path(&package_root, absolute_path).ok()?;
    if relative_path.is_empty()
        || relative_path == ".."
        || relative_path.starts_with("../")
        || node_is_absolute(&relative_path)
    {
        return None;
    }
    let label = relative_path.replace("\\", "/");
    if label == "README.md" || label.starts_with("docs/") || label.starts_with("examples/") {
        return Some(CompactReadClassification {
            kind: CompactReadKind::Docs,
            label,
        });
    }
    None
}

fn get_compact_read_classification(args: &JsValue, cwd: &JsString) -> Option<CompactReadClassification> {
    let raw_path = render_args_path(args)?;
    if raw_path.is_empty() {
        return None;
    }
    let absolute_path = super::path_utils::resolve_to_cwd(&raw_path, cwd).ok()?;
    let file_name = node_basename(&absolute_path);
    if file_name == "SKILL.md" {
        let directory_name = node_basename(node_dirname(&absolute_path));
        let label = if directory_name.is_empty() {
            file_name.clone()
        } else {
            directory_name
        };
        return Some(CompactReadClassification {
            kind: CompactReadKind::Skill,
            label,
        });
    }
    if let Some(classification) = get_pi_docs_classification(&absolute_path) {
        return Some(classification);
    }
    if COMPACT_RESOURCE_FILE_NAMES.iter().any(|name| file_name == *name) {
        return Some(CompactReadClassification {
            kind: CompactReadKind::Resource,
            label: format_path_relative_to_cwd_or_absolute(&absolute_path, cwd).unwrap_or(absolute_path),
        });
    }
    None
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
        theme.fg(ThemeColor::ToolTitle, &theme.bold(&format!("read {kind}"))),
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
) -> JsString {
    if !options.expanded && !is_error {
        return JsString::default();
    }
    let raw_path = render_args_path(args);
    let output = get_text_output(Some(result), show_images);
    let lang = if !is_error {
        raw_path.as_ref().and_then(get_language_from_path).map(JsString::from)
    } else {
        None
    };
    let rendered_lines = match lang.as_ref() {
        Some(lang) => highlight_code(&replace_tabs(&output), Some(lang)),
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
                None => theme.fg(ThemeColor::ToolOutput, &replace_tabs(line)),
            }),
            "\n"
        )
    );
    if remaining > 0 {
        text.push_str(js_format!(
            "{}{}{}",
            theme.fg(ThemeColor::Muted, &format!("\n... ({remaining} more lines,")),
            key_hint("app.tools.expand", "to expand"),
            theme.fg(ThemeColor::Muted, ")")
        ));
    }
    if let Some(truncation) = result
        .details
        .as_ref()
        .and_then(|details| details.get("truncation"))
        .and_then(|value| pi_js_compat::js_json::from_value::<TruncationResult>(value).ok())
        && truncation.truncated
    {
        if truncation.first_line_exceeds_limit {
            text.push_str(js_format!(
                "\n{}",
                theme.fg(
                    ThemeColor::Warning,
                    &format!("[First line exceeds {} limit]", format_size(truncation.max_bytes))
                )
            ));
        } else if truncation.truncated_by == Some(TruncatedBy::Lines) {
            text.push_str(js_format!(
                "\n{}",
                theme.fg(
                    ThemeColor::Warning,
                    &format!(
                        "[Truncated: showing {} of {} lines ({} line limit)]",
                        truncation.output_lines, truncation.total_lines, truncation.max_lines
                    )
                )
            ));
        } else {
            text.push_str(js_format!(
                "\n{}",
                theme.fg(
                    ThemeColor::Warning,
                    &format!(
                        "[Truncated: {} lines shown ({} limit)]",
                        truncation.output_lines,
                        format_size(truncation.max_bytes)
                    )
                )
            ));
        }
    }
    text
}

/// `renderCall` for the read tool.
pub fn render_call(args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    let classification = if context.expanded {
        None
    } else {
        get_compact_read_classification(args, &context.cwd)
    };
    let text = match classification {
        Some(classification) => format_compact_read_call(&classification, args, theme),
        None => format_read_call(args, theme, &context.cwd),
    };
    Rc::new(RefCell::new(Text::new(text, 0, 0)))
}

/// `renderResult` for the read tool.
pub fn render_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    let text = format_read_result(
        &context.args,
        result,
        options,
        theme,
        context.show_images,
        context.is_error,
    );
    Rc::new(RefCell::new(Text::new(text, 0, 0)))
}

/// `createReadToolDefinition`: the read tool plus its renderers.
pub fn create_read_tool_definition(
    cwd: impl Into<JsString>,
    options: ReadToolOptions,
) -> crate::core::extensions::types::ToolDefinition {
    let tool: std::sync::Arc<dyn AgentTool> = std::sync::Arc::new(ReadTool::with_options(cwd, options));
    let mut definition = super::tool_definition_wrapper::create_tool_definition_from_agent_tool(tool);
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
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

/// `createReadTool`: the read tool wrapped with the shared result cap.
pub fn create_read_tool(cwd: impl Into<JsString>, options: ReadToolOptions) -> std::sync::Arc<dyn AgentTool> {
    super::tool_definition_wrapper::wrap_agent_tool(std::sync::Arc::new(ReadTool::with_options(cwd, options)))
}
