//! Image conversion to PNG for terminal display.
//!
//! The source awaits `loadPhoton()`; the Rust port links the image backend
//! directly, so conversion is synchronous.

use pi_agent_core::harness::tools::image::{decode_base64, encode_base64};

use crate::utils::exif_orientation::apply_exif_orientation;
use crate::utils::photon::{decode_image, png_bytes};

/// A base64 payload with its MIME type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertedImage {
    pub data: String,
    pub mime_type: String,
}

/// Converts image bytes to PNG, applying EXIF orientation.
pub fn convert_image_bytes_to_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = decode_image(bytes).ok()?;
    let image = apply_exif_orientation(image, bytes);
    png_bytes(&image).ok()
}

/// Converts a base64 payload to PNG; PNG input passes through unchanged.
pub fn convert_to_png(base64_data: &str, mime_type: &str) -> Option<ConvertedImage> {
    if mime_type == "image/png" {
        return Some(ConvertedImage {
            data: base64_data.to_owned(),
            mime_type: mime_type.to_owned(),
        });
    }

    let png = convert_image_bytes_to_png(&decode_base64(base64_data))?;
    Some(ConvertedImage {
        data: encode_base64(&png),
        mime_type: "image/png".to_owned(),
    })
}
