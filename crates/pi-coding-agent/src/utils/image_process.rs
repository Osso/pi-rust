//! Image normalization for inline provider limits.
//!
//! The source awaits `resizeImage` (a worker thread). The Rust port runs the
//! whole pipeline on one blocking thread: converting unsupported formats and
//! resizing are both CPU-bound, and splitting them across two blocking tasks
//! would only add hand-offs.

use pi_agent_core::harness::tools::image::encode_base64;

use crate::utils::image_convert::convert_image_bytes_to_png;
use crate::utils::image_resize::format_dimension_note;
use crate::utils::image_resize_core::{ImageResizeOptions, resize_image_in_process};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessImageOptions {
    /// Whether to resize images to inline provider limits. Default: true.
    pub auto_resize_images: Option<bool>,
    /// Resize overrides; defaults apply when omitted.
    pub resize_options: Option<ImageResizeOptions>,
}

/// Successful processing, or the message replacing an omitted image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessImageResult {
    Processed {
        data: String,
        mime_type: String,
        hints: Vec<String>,
    },
    Omitted {
        message: String,
    },
}

struct NormalizedImage {
    bytes: Vec<u8>,
    mime_type: String,
    converted_from: Option<String>,
}

/// Normalizes an image for inline provider requests.
pub async fn process_image(bytes: &[u8], mime_type: &str, options: Option<ProcessImageOptions>) -> ProcessImageResult {
    let options = options.unwrap_or_default();
    let bytes = bytes.to_vec();
    let mime_type = mime_type.to_owned();
    match tokio::task::spawn_blocking(move || process_image_blocking(&bytes, &mime_type, options)).await {
        Ok(result) => result,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

fn process_image_blocking(bytes: &[u8], mime_type: &str, options: ProcessImageOptions) -> ProcessImageResult {
    let Some(normalized) = normalize_image(bytes, mime_type) else {
        return ProcessImageResult::Omitted {
            message: "[Image omitted: could not be converted to a supported inline image format.]".to_owned(),
        };
    };

    if options.auto_resize_images.unwrap_or(true) {
        let Some(resized) = resize_image_in_process(&normalized.bytes, &normalized.mime_type, options.resize_options)
        else {
            return ProcessImageResult::Omitted {
                message: "[Image omitted: could not be resized below the inline image size limit.]".to_owned(),
            };
        };

        let mut hints = Vec::new();
        if let Some(hint) = conversion_hint(normalized.converted_from.as_deref(), &resized.mime_type) {
            hints.push(hint);
        }
        if let Some(note) = format_dimension_note(&resized) {
            hints.push(note);
        }

        return ProcessImageResult::Processed {
            data: resized.data,
            mime_type: resized.mime_type,
            hints,
        };
    }

    let hints = conversion_hint(normalized.converted_from.as_deref(), &normalized.mime_type)
        .into_iter()
        .collect();
    ProcessImageResult::Processed {
        data: encode_base64(&normalized.bytes),
        mime_type: normalized.mime_type,
        hints,
    }
}

fn normalize_image(bytes: &[u8], mime_type: &str) -> Option<NormalizedImage> {
    if let Some(normalized_mime_type) = normalize_supported_image_mime_type(mime_type) {
        return Some(NormalizedImage {
            bytes: bytes.to_vec(),
            mime_type: normalized_mime_type.to_owned(),
            converted_from: None,
        });
    }

    let png = convert_image_bytes_to_png(bytes)?;
    Some(NormalizedImage {
        bytes: png,
        mime_type: "image/png".to_owned(),
        converted_from: Some(base_mime_type(mime_type)),
    })
}

fn normalize_supported_image_mime_type(mime_type: &str) -> Option<&'static str> {
    match base_mime_type(mime_type).as_str() {
        "image/png" => Some("image/png"),
        "image/jpeg" | "image/jpg" => Some("image/jpeg"),
        "image/gif" => Some("image/gif"),
        "image/webp" => Some("image/webp"),
        _ => None,
    }
}

fn base_mime_type(mime_type: &str) -> String {
    mime_type.split(';').next().unwrap_or(mime_type).trim().to_lowercase()
}

fn conversion_hint(from: Option<&str>, to: &str) -> Option<String> {
    let from = from?;
    (from != to).then(|| format!("[Image converted from {from} to {to}.]"))
}
