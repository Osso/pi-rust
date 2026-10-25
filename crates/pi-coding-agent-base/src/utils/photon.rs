//! Photon image processing wrapper.
//!
//! The pinned source loads `@silvia-odwyer/photon-node` (a WASM build of
//! `photon-rs`) at runtime and patches `fs.readFileSync` so Bun executables can
//! find the WASM module. The Rust port links `photon-rs` directly, so loading
//! cannot fail and every caller has the image backend available.
//!
//! `photon-rs` only exposes panicking decode/encode wrappers. The wrappers here
//! perform the same `image` crate calls but return `Result`, so a malformed
//! image produces the source's `null` result instead of aborting the process.

use std::io::Cursor;

use image::{DynamicImage, ImageBuffer, ImageOutputFormat, RgbaImage, load_from_memory};
use photon_rs::PhotonImage;

pub use photon_rs::transform::SamplingFilter;

/// Filter used by every resize in the pinned source (`SamplingFilter.Lanczos3`).
pub const RESIZE_FILTER: SamplingFilter = SamplingFilter::Lanczos3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhotonError {
    /// `PhotonImage.new_from_byteslice` could not decode the input.
    Decode { message: String },
    /// Encoding raw pixels failed (for example a mismatched pixel buffer).
    Encode { message: String },
}

impl std::fmt::Display for PhotonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode { message } => write!(formatter, "image decode failed: {message}"),
            Self::Encode { message } => write!(formatter, "image encode failed: {message}"),
        }
    }
}

impl std::error::Error for PhotonError {}

/// Decodes image bytes to RGBA pixels, mirroring `PhotonImage.new_from_byteslice`.
pub fn decode_image(bytes: &[u8]) -> Result<PhotonImage, PhotonError> {
    let image = load_from_memory(bytes).map_err(|error| PhotonError::Decode {
        message: error.to_string(),
    })?;
    let width = image.width();
    let height = image.height();
    Ok(PhotonImage::new(image.to_rgba8().into_raw(), width, height))
}

/// Encodes the image as PNG, mirroring `PhotonImage.get_bytes`.
pub fn png_bytes(image: &PhotonImage) -> Result<Vec<u8>, PhotonError> {
    encode(image, ImageOutputFormat::Png)
}

/// Encodes the image as JPEG at `quality`, mirroring `PhotonImage.get_bytes_jpeg`.
pub fn jpeg_bytes(image: &PhotonImage, quality: u8) -> Result<Vec<u8>, PhotonError> {
    encode(image, ImageOutputFormat::Jpeg(quality))
}

fn encode(image: &PhotonImage, format: ImageOutputFormat) -> Result<Vec<u8>, PhotonError> {
    let mut buffer = Vec::new();
    dynamic_image(image)?
        .write_to(&mut Cursor::new(&mut buffer), format)
        .map_err(|error| PhotonError::Encode {
            message: error.to_string(),
        })?;
    Ok(buffer)
}

fn dynamic_image(image: &PhotonImage) -> Result<DynamicImage, PhotonError> {
    let buffer: RgbaImage = ImageBuffer::from_raw(image.get_width(), image.get_height(), image.get_raw_pixels())
        .ok_or_else(|| PhotonError::Encode {
            message: "pixel buffer does not match the image dimensions".to_owned(),
        })?;
    Ok(DynamicImage::ImageRgba8(buffer))
}
