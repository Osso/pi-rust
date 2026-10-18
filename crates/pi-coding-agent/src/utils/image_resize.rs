//! Image resizing off the event loop.
//!
//! The source runs Photon in a `worker_threads` worker and falls back to
//! in-process resizing when the worker cannot load. The Rust port uses
//! `tokio::task::spawn_blocking`: the backend is statically linked, so there is
//! no worker-load failure mode and no fallback path. A panic inside the
//! blocking task is re-raised on the caller, mirroring an unhandled worker
//! failure rather than silently changing behavior.

use tokio::task::spawn_blocking;

use crate::utils::image_resize_core::{ImageResizeOptions, ResizedImage, resize_image_in_process};

/// Resizes an image on a blocking thread.
pub async fn resize_image(
    input_bytes: &[u8],
    mime_type: &str,
    options: Option<ImageResizeOptions>,
) -> Result<Option<ResizedImage>, tokio::task::JoinError> {
    let bytes = input_bytes.to_vec();
    let mime_type = mime_type.to_owned();
    match spawn_blocking(move || resize_image_in_process(&bytes, &mime_type, options)).await {
        Ok(result) => Ok(result),
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

/// Formats the coordinate-mapping note for resized images.
pub fn format_dimension_note(result: &ResizedImage) -> Option<String> {
    if !result.was_resized {
        return None;
    }

    Some(format!(
        "[Image: original {}x{}, displayed at {}x{}. Multiply coordinates by {} to map to original image.]",
        result.original_width,
        result.original_height,
        result.width,
        result.height,
        scale_factor(result.original_width, result.width)
    ))
}

/// `(originalWidth / width).toFixed(2)`.
///
/// Computed with integer arithmetic: ECMAScript rounds to the nearest
/// hundredth and picks the larger value on an exact tie, which Rust's
/// half-to-even `{:.2}` formatting does not do (for example `9 / 8`).
fn scale_factor(original_width: u32, width: u32) -> String {
    let hundredths = (200 * u64::from(original_width) + u64::from(width)) / (2 * u64::from(width));
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}
