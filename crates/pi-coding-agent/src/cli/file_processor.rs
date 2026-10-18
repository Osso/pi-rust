//! `@file` CLI arguments (`cli/file-processor.ts`).

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use pi_ai::types::ImageContent;

use crate::core::tools::path_utils::resolve_read_path;
use crate::utils::image_process::{ProcessImageOptions, ProcessImageResult, process_image};
use crate::utils::mime::detect_supported_image_mime_type_from_file;

/// Text and image attachments produced from `@file` arguments.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcessedFiles {
    pub text: String,
    pub images: Vec<ImageContent>,
}

/// `ProcessFileOptions`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessFileOptions {
    /// Whether to auto-resize images to 2000x2000 max. Default: true.
    pub auto_resize_images: Option<bool>,
}

/// Failure that the source reports on stderr before `process.exit(1)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProcessorError {
    message: String,
}

impl FileProcessorError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for FileProcessorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for FileProcessorError {}

/// Process `@file` arguments into text content and image attachments.
pub async fn process_file_arguments(
    file_args: &[String],
    options: Option<ProcessFileOptions>,
    cwd: &str,
) -> Result<ProcessedFiles, FileProcessorError> {
    let auto_resize_images = options.and_then(|options| options.auto_resize_images);
    let mut text = String::new();
    let mut images = Vec::new();

    for file_arg in file_args {
        let absolute_path =
            resolve_read_path(file_arg, cwd).map_err(|error| FileProcessorError::new(error.to_string()))?;
        let metadata = tokio::fs::metadata(&absolute_path)
            .await
            .map_err(|_| FileProcessorError::new(format!("File not found: {absolute_path}")))?;
        if metadata.len() == 0 {
            continue;
        }

        let mime_type = detect_supported_image_mime_type_from_file(Path::new(&absolute_path))
            .map_err(|error| FileProcessorError::new(error.to_string()))?;
        match mime_type {
            Some(mime_type) => {
                process_image_argument(&absolute_path, &mime_type, auto_resize_images, &mut text, &mut images).await?;
            }
            None => {
                let content = tokio::fs::read(&absolute_path).await.map_err(|error| {
                    FileProcessorError::new(format!("Could not read file {absolute_path}: {error}"))
                })?;
                text.push_str(&format!(
                    "<file name=\"{absolute_path}\">\n{}\n</file>\n",
                    String::from_utf8_lossy(&content)
                ));
            }
        }
    }

    Ok(ProcessedFiles { text, images })
}

async fn process_image_argument(
    absolute_path: &str,
    mime_type: &str,
    auto_resize_images: Option<bool>,
    text: &mut String,
    images: &mut Vec<ImageContent>,
) -> Result<(), FileProcessorError> {
    let content = tokio::fs::read(absolute_path)
        .await
        .map_err(|error| FileProcessorError::new(format!("Could not read file {absolute_path}: {error}")))?;
    let processed = process_image(
        &content,
        mime_type,
        Some(ProcessImageOptions {
            auto_resize_images,
            resize_options: None,
        }),
    )
    .await;

    match processed {
        ProcessImageResult::Omitted { message } => {
            text.push_str(&format!("<file name=\"{absolute_path}\">{message}</file>\n"));
        }
        ProcessImageResult::Processed { data, mime_type, hints } => {
            images.push(ImageContent::new(data, mime_type));
            let hints = hints.join("\n");
            text.push_str(&format!("<file name=\"{absolute_path}\">{hints}</file>\n"));
        }
    }
    Ok(())
}
