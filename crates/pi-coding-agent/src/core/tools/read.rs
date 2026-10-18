//! Ported from `packages/coding-agent/src/core/tools/read.ts`.
//!
//! Text offsets/limits, oversized head-range reads, and image attachment
//! normalization. Renderers are pending `theme.ts`.

use std::io;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{ImageContent, TextContent, TextOrImageContent};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::sync::CancellationToken;

use super::node_fs::node_fs_error;
use super::path_utils::resolve_read_path_async;
use super::truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncatedBy, TruncationOptions, TruncationResult, format_size, truncate_head,
};
use crate::utils::image_process::{ProcessImageOptions, ProcessImageResult, process_image};
use crate::utils::mime::detect_supported_image_mime_type_from_file;

const DESCRIPTION: &str = "Read the contents of a file. Supports text files and images (jpg, png, gif, webp, bmp). Images are sent as attachments. For text files, output is truncated to 2000 lines or 10KB (whichever is hit first). Use offset/limit for large files. When you need the full file, continue with offset until complete.";

/// File access used by [`ReadTool`]. Override to delegate reads to a remote system.
pub trait ReadOperations: Send + Sync {
    fn read_file<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<Vec<u8>>>;

    /// Whether [`ReadOperations::read_range`] is implemented.
    fn supports_read_range(&self) -> bool {
        false
    }

    /// Reads `[start, end)` bytes. Only called when [`ReadOperations::supports_read_range`] is true.
    fn read_range<'a>(&'a self, absolute_path: &'a str, start: u64, end: u64) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        let _ = (absolute_path, start, end);
        Box::pin(async { Err(io::Error::other("readRange is not implemented")) })
    }

    /// File size, or `None` when the operation does not expose `stat`.
    fn stat<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<Option<u64>>> {
        let _ = absolute_path;
        Box::pin(async { Ok(None) })
    }

    fn access<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<()>>;

    /// Detects an image MIME type; `None` for non-images.
    fn detect_image_mime_type<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<Option<String>>> {
        let _ = absolute_path;
        Box::pin(async { Ok(None) })
    }
}

/// Local filesystem operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalReadOperations;

impl ReadOperations for LocalReadOperations {
    fn read_file<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async move { tokio::fs::read(absolute_path).await })
    }

    fn supports_read_range(&self) -> bool {
        true
    }

    fn read_range<'a>(&'a self, absolute_path: &'a str, start: u64, end: u64) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async move {
            let length = end.saturating_sub(start);
            let mut file = tokio::fs::File::open(absolute_path).await?;
            file.seek(io::SeekFrom::Start(start)).await?;
            let mut buffer = vec![0; length as usize];
            let mut bytes_read = 0;
            while bytes_read < buffer.len() {
                let read = file.read(&mut buffer[bytes_read..]).await?;
                if read == 0 {
                    break;
                }
                bytes_read += read;
            }
            buffer.truncate(bytes_read);
            Ok(buffer)
        })
    }

    fn stat<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<Option<u64>>> {
        Box::pin(async move {
            tokio::fs::metadata(absolute_path)
                .await
                .map(|metadata| Some(metadata.len()))
        })
    }

    fn access<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            tokio::fs::metadata(absolute_path).await?;
            Ok(())
        })
    }

    fn detect_image_mime_type<'a>(&'a self, absolute_path: &'a str) -> BoxFuture<'a, io::Result<Option<String>>> {
        Box::pin(async move {
            detect_supported_image_mime_type_from_file(std::path::Path::new(absolute_path))
                .map(|mime_type| mime_type.map(|value| value.to_owned()))
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
    cwd: String,
    parameters: Value,
    operations: std::sync::Arc<dyn ReadOperations>,
    auto_resize_images: bool,
}

impl ReadTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_options(cwd, ReadToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<String>, options: ReadToolOptions) -> Self {
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

fn read_schema() -> Value {
    json!({
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

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "read"
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
        let operations = std::sync::Arc::clone(&self.operations);
        let auto_resize_images = self.auto_resize_images;
        Box::pin(async move { execute_read(&cwd, operations, auto_resize_images, params, signal).await })
    }
}

struct ReadInput {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

async fn execute_read(
    cwd: &str,
    operations: std::sync::Arc<dyn ReadOperations>,
    auto_resize_images: bool,
    params: Value,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let input = parse_read_input(&params)?;
    check_aborted(signal.as_ref())?;
    let absolute_path = resolve_read_path_async(&input.path, cwd)
        .await
        .map_err(|error| ToolError::new(error.to_string()))?;
    operations
        .access(&absolute_path)
        .await
        .map_err(|error| ToolError::new(node_fs_error(&error, "access", &absolute_path)))?;
    check_aborted(signal.as_ref())?;

    let mime_type = operations
        .detect_image_mime_type(&absolute_path)
        .await
        .map_err(|error| ToolError::new(node_fs_error(&error, "open", &absolute_path)))?;

    let (content, details) = match mime_type {
        Some(mime_type) => read_image(operations.as_ref(), &absolute_path, &mime_type, auto_resize_images).await?,
        None => read_text(operations.as_ref(), &absolute_path, &input).await?,
    };
    check_aborted(signal.as_ref())?;
    Ok(AgentToolResult {
        content,
        details,
        usage: None,
        added_tool_names: None,
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

fn parse_read_input(params: &Value) -> Result<ReadInput, ToolError> {
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("read tool requires a string path"))?
        .to_owned();
    Ok(ReadInput {
        path,
        offset: number_argument(params, "offset"),
        limit: number_argument(params, "limit"),
    })
}

/// JavaScript `Number` to array-index conversion: truncate toward zero, non-finite as 0.
fn number_argument(params: &Value, name: &str) -> Option<usize> {
    let value = params.get(name)?.as_f64()?;
    if !value.is_finite() || value <= 0.0 {
        return Some(0);
    }
    Some(value.trunc() as usize)
}

async fn read_image(
    operations: &dyn ReadOperations,
    absolute_path: &str,
    mime_type: &str,
    auto_resize_images: bool,
) -> Result<(Vec<TextOrImageContent>, Option<Value>), ToolError> {
    let buffer = operations
        .read_file(absolute_path)
        .await
        .map_err(|error| ToolError::new(node_fs_error(&error, "open", absolute_path)))?;
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
    absolute_path: &str,
    input: &ReadInput,
) -> Result<(Vec<TextOrImageContent>, Option<Value>), ToolError> {
    let file_stat = operations
        .stat(absolute_path)
        .await
        .map_err(|error| ToolError::new(node_fs_error(&error, "stat", absolute_path)))?;
    let range_limited = can_use_head_range_read(file_stat, operations, input.offset, input.limit);
    let buffer = if range_limited {
        operations
            .read_range(absolute_path, 0, DEFAULT_MAX_BYTES as u64)
            .await
            .map_err(|error| ToolError::new(node_fs_error(&error, "read", absolute_path)))?
    } else {
        operations
            .read_file(absolute_path)
            .await
            .map_err(|error| ToolError::new(node_fs_error(&error, "open", absolute_path)))?
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
    path: &'a str,
    start_line_display: usize,
    total_file_lines: usize,
    all_lines: &'a [&'a str],
    user_limited_lines: Option<usize>,
    truncation: &'a TruncationResult,
    range_limited: bool,
    file_stat: Option<u64>,
}

fn render_read_output(input: &ReadRenderInput<'_>) -> (String, Option<Value>) {
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
        let output_text = format!(
            "{}\n\n[Showing lines {}-{} of file ({} limit). Use offset={} to continue.]",
            truncation.content,
            start_line_display,
            end_line_display,
            format_size(DEFAULT_MAX_BYTES),
            next_offset
        );
        return (output_text, Some(truncation_details(truncation, true, file_stat)));
    }
    if truncation.first_line_exceeds_limit {
        let first_line_size = format_size(all_lines[start_line_display - 1].len());
        let output_text = format!(
            "[Line {} is {}, exceeds {} limit. Use bash: sed -n '{}p' {} | head -c {}]",
            start_line_display,
            first_line_size,
            format_size(DEFAULT_MAX_BYTES),
            start_line_display,
            path,
            DEFAULT_MAX_BYTES
        );
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
                format!(
                    "{}\n\n[{} more lines in file. Use offset={} to continue.]",
                    truncation.content, remaining, next_offset
                ),
                None,
            );
        }
    }
    (truncation.content.clone(), None)
}

fn truncation_details(truncation: &TruncationResult, force_bytes: bool, total_bytes: Option<u64>) -> Value {
    let mut value = serde_json::to_value(truncation).expect("truncation metadata must serialize");
    if let Value::Object(details) = &mut value
        && force_bytes
    {
        details.insert("truncated".to_owned(), Value::Bool(true));
        details.insert("truncatedBy".to_owned(), json!("bytes"));
        if let Some(total_bytes) = total_bytes {
            details.insert("totalBytes".to_owned(), json!(total_bytes));
        }
    }
    json!({"truncation": value})
}
