use std::sync::Arc;

use pi_ai::request::BoxFuture;
use pi_ai::types::{ImageContent, TextContent, TextOrImageContent};
use serde_json::{Value, json};

use crate::agent::{AgentTool, AgentToolResult, ToolError};
use crate::harness::tools::image::{detect_supported_image_mime_type, encode_base64};
use crate::harness::tools::path_utils::resolve_read_tool_path;
use crate::harness::tools::tool_context::ExecutionToolContext;
use crate::harness::utils::{DEFAULT_MAX_BYTES, TruncationOptions, format_size, truncate_head};
use tokio_util::sync::CancellationToken;

const DESCRIPTION: &str = "Read the contents of a file. Supports text files and images (jpg, png, gif, webp, bmp). Images are sent as attachments. For text files, output is truncated to 2000 lines or 10KB (whichever is hit first). Use offset/limit for large files. When you need the full file, continue with offset until complete.";

/// Options for [`ReadTool`].
#[derive(Clone, Default)]
pub struct ReadToolOptions {
    /// Whether an injected image processor should resize images. Default: `true`.
    pub auto_resize_images: Option<bool>,
    /// Optional image conversion/resizing implementation.
    pub image_processor: Option<Arc<dyn ReadImageProcessor>>,
}

/// Options passed to a [`ReadImageProcessor`].
#[derive(Debug, Clone, Copy)]
pub struct ImageProcessorOptions {
    pub auto_resize_images: bool,
}

/// Result returned by a [`ReadImageProcessor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadImageProcessorResult {
    Ok {
        data: String,
        mime_type: String,
        hints: Vec<String>,
    },
    Err {
        message: String,
    },
}

/// Optional image conversion/resizing implementation.
pub trait ReadImageProcessor: Send + Sync {
    fn process<'a>(
        &'a self,
        bytes: &'a [u8],
        mime_type: &'a str,
        options: ImageProcessorOptions,
    ) -> BoxFuture<'a, ReadImageProcessorResult>;
}

/// Reads text files and images from the execution environment.
pub struct ReadTool {
    context: Arc<ExecutionToolContext>,
    parameters: Value,
    options: ReadToolOptions,
}

impl ReadTool {
    pub fn new(context: Arc<ExecutionToolContext>) -> Self {
        Self::with_options(context, ReadToolOptions::default())
    }

    pub fn with_options(context: Arc<ExecutionToolContext>, options: ReadToolOptions) -> Self {
        Self {
            context,
            parameters: read_schema(),
            options,
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
        _on_update: crate::agent::AgentToolUpdateCallback,
        _execution: Option<crate::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let context = Arc::clone(&self.context);
        let options = self.options.clone();
        Box::pin(async move { execute_read(context, options, params, signal).await })
    }
}

async fn execute_read(
    context: Arc<ExecutionToolContext>,
    options: ReadToolOptions,
    params: Value,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("read tool requires a string path"))?
        .to_owned();
    let offset = params.get("offset").and_then(Value::as_u64).map(|value| value as usize);
    let limit = params.get("limit").and_then(Value::as_u64).map(|value| value as usize);

    let absolute_path = resolve_read_tool_path(context.env.as_ref(), &path, signal.as_ref())
        .await
        .map_err(|error| ToolError::new(error.message))?;
    let bytes = context
        .env
        .read_binary_file(&absolute_path, signal.as_ref())
        .await
        .map_err(|error| ToolError::new(error.message))?;

    if let Some(mime_type) = detect_supported_image_mime_type(&bytes) {
        return read_image(options, &bytes, mime_type).await;
    }

    let text_content = String::from_utf8_lossy(&bytes);
    let all_lines = text_content.split('\n').collect::<Vec<_>>();
    let total_file_lines = all_lines.len();
    let start_line = offset.map_or(0, |offset| offset.saturating_sub(1));
    let start_line_display = start_line + 1;
    if start_line >= all_lines.len() {
        return Err(ToolError::new(format!(
            "Offset {} is beyond end of file ({} lines total)",
            offset.unwrap_or(0),
            all_lines.len()
        )));
    }

    let (selected_content, user_limited_lines) = match limit {
        Some(limit) => {
            let end_line = (start_line + limit).min(all_lines.len());
            (all_lines[start_line..end_line].join("\n"), Some(end_line - start_line))
        }
        None => (all_lines[start_line..].join("\n"), None),
    };

    let truncation = truncate_head(&selected_content, TruncationOptions::default());
    let mut details = None;
    let output_text = if truncation.first_line_exceeds_limit {
        let first_line_size = format_size(all_lines[start_line].len());
        details = Some(json!({"truncation": truncation}));
        format!(
            "[Line {} is {}, exceeds {} limit. Use bash: sed -n '{}p' {} | head -c {}]",
            start_line_display,
            first_line_size,
            format_size(DEFAULT_MAX_BYTES),
            start_line_display,
            path,
            DEFAULT_MAX_BYTES
        )
    } else if truncation.truncated {
        let end_line_display = start_line_display + truncation.output_lines - 1;
        let next_offset = end_line_display + 1;
        let mut text = truncation.content.clone();
        if truncation.truncated_by == Some(crate::harness::utils::TruncatedBy::Lines) {
            text.push_str(&format!(
                "\n\n[Showing lines {}-{} of {}. Use offset={} to continue.]",
                start_line_display, end_line_display, total_file_lines, next_offset
            ));
        } else {
            text.push_str(&format!(
                "\n\n[Showing lines {}-{} of {} ({} limit). Use offset={} to continue.]",
                start_line_display,
                end_line_display,
                total_file_lines,
                format_size(DEFAULT_MAX_BYTES),
                next_offset
            ));
        }
        details = Some(json!({"truncation": truncation}));
        text
    } else if let Some(user_limited_lines) = user_limited_lines {
        if start_line + user_limited_lines < all_lines.len() {
            let remaining = all_lines.len() - (start_line + user_limited_lines);
            let next_offset = start_line + user_limited_lines + 1;
            format!(
                "{}\n\n[{} more lines in file. Use offset={} to continue.]",
                truncation.content, remaining, next_offset
            )
        } else {
            truncation.content
        }
    } else {
        truncation.content
    };

    Ok(AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(output_text))],
        details,
        usage: None,
        added_tool_names: None,
        is_error: None,
        terminate: None,
    })
}

async fn read_image(
    options: ReadToolOptions,
    bytes: &[u8],
    mime_type: &'static str,
) -> Result<AgentToolResult, ToolError> {
    if let Some(processor) = &options.image_processor {
        let processed = processor
            .process(
                bytes,
                mime_type,
                ImageProcessorOptions {
                    auto_resize_images: options.auto_resize_images.unwrap_or(true),
                },
            )
            .await;
        return Ok(match processed {
            ReadImageProcessorResult::Err { message } => {
                text_result(format!("Read image file [{mime_type}]\n{message}"))
            }
            ReadImageProcessorResult::Ok { data, mime_type, hints } => {
                let hints = if hints.is_empty() {
                    String::new()
                } else {
                    format!("\n{}", hints.join("\n"))
                };
                AgentToolResult {
                    content: vec![
                        TextOrImageContent::Text(TextContent::new(format!("Read image file [{mime_type}]{hints}"))),
                        TextOrImageContent::Image(ImageContent::new(data, mime_type)),
                    ],
                    details: None,
                    usage: None,
                    added_tool_names: None,
                    is_error: None,
                    terminate: None,
                }
            }
        });
    }

    if mime_type == "image/bmp" {
        return Ok(text_result(
            "Read image file [image/bmp]\n[Image omitted: configure an imageProcessor to convert BMP images.]"
                .to_owned(),
        ));
    }
    Ok(AgentToolResult {
        content: vec![
            TextOrImageContent::Text(TextContent::new(format!("Read image file [{mime_type}]"))),
            TextOrImageContent::Image(ImageContent::new(encode_base64(bytes), mime_type)),
        ],
        details: None,
        usage: None,
        added_tool_names: None,
        is_error: None,
        terminate: None,
    })
}

fn text_result(text: String) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: None,
        usage: None,
        added_tool_names: None,
        is_error: None,
        terminate: None,
    }
}
