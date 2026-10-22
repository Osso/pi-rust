//! `@file` CLI arguments (`cli/file-processor.ts`).

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use pi_ai::types::ImageContent;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

use crate::core::tools::path_utils::resolve_read_path;
use crate::utils::image_process::{ProcessImageOptions, ProcessImageResult, process_image};
use crate::utils::mime::detect_supported_image_mime_type_from_file;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcessedFiles {
    pub text: JsString,
    pub images: Vec<ImageContent>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessFileOptions {
    pub auto_resize_images: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProcessorError {
    message: JsString,
}
impl FileProcessorError {
    fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
        }
    }
    pub fn message(&self) -> &JsString {
        &self.message
    }
}
impl Display for FileProcessorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}
impl Error for FileProcessorError {}

/// Process original JS paths, or already-decoded CLI argv strings, through one native path authority.
pub async fn process_file_arguments(
    file_args: &[impl Clone + Into<JsString>],
    options: Option<ProcessFileOptions>,
    cwd: &JsString,
) -> Result<ProcessedFiles, FileProcessorError> {
    let auto_resize_images = options.and_then(|options| options.auto_resize_images);
    let mut result = ProcessedFiles::default();
    for file_arg in file_args {
        let path = resolve_read_path(file_arg.clone(), cwd)
            .map_err(|error| FileProcessorError::new(error.message().clone()))?;
        let attachment = read_file_argument(&path, auto_resize_images).await?;
        result.text.push_str(attachment.text);
        result.images.extend(attachment.images);
    }
    Ok(result)
}

async fn read_file_argument(
    path: &JsString,
    auto_resize_images: Option<bool>,
) -> Result<ProcessedFiles, FileProcessorError> {
    let metadata = tokio::fs::metadata(encode_node_utf8(path))
        .await
        .map_err(|_| FileProcessorError::new(js_format!("File not found: {}", path)))?;
    if metadata.len() == 0 {
        return Ok(ProcessedFiles::default());
    }
    let mime_type = detect_supported_image_mime_type_from_file(Path::new(&encode_node_utf8(path)))
        .map_err(|error| FileProcessorError::new(error.to_string()))?;
    let content = tokio::fs::read(encode_node_utf8(path))
        .await
        .map_err(|error| FileProcessorError::new(js_format!("Could not read file {}: {}", path, error.to_string())))?;
    if let Some(mime_type) = mime_type {
        return Ok(process_image_argument(path, &content, mime_type, auto_resize_images).await);
    }
    Ok(ProcessedFiles {
        text: js_format!(
            "<file name=\"{}\">\n{}\n</file>\n",
            path,
            String::from_utf8_lossy(&content).into_owned()
        ),
        images: Vec::new(),
    })
}

async fn process_image_argument(
    path: &JsString,
    content: &[u8],
    mime_type: &str,
    auto_resize_images: Option<bool>,
) -> ProcessedFiles {
    let processed = process_image(
        content,
        mime_type,
        Some(ProcessImageOptions {
            auto_resize_images,
            resize_options: None,
        }),
    )
    .await;
    match processed {
        ProcessImageResult::Omitted { message } => ProcessedFiles {
            text: js_format!("<file name=\"{}\">{}</file>\n", path, message),
            images: Vec::new(),
        },
        ProcessImageResult::Processed { data, mime_type, hints } => ProcessedFiles {
            text: js_format!("<file name=\"{}\">{}</file>\n", path, hints.join("\n")),
            images: vec![ImageContent::new(data, mime_type)],
        },
    }
}
