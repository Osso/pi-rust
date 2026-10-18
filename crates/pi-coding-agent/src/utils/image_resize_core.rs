//! In-process image resizing to inline provider limits.
//!
//! Strategy (from the pinned source): resize to the requested bounds, encode
//! PNG and JPEG at several quality steps, take the first candidate below the
//! encoded-size limit, then shrink dimensions by 25% until 1x1.

use photon_rs::PhotonImage;
use photon_rs::transform::resize as photon_resize;
use pi_agent_core::harness::tools::image::encode_base64;
use serde::{Deserialize, Serialize};

use crate::utils::exif_orientation::apply_exif_orientation;
use crate::utils::photon::{RESIZE_FILTER, decode_image, jpeg_bytes, png_bytes};

pub const DEFAULT_MAX_WIDTH: u32 = 2000;
pub const DEFAULT_MAX_HEIGHT: u32 = 2000;
/// 4.5MB of base64 payload, below Anthropic's 5MB limit.
pub const DEFAULT_MAX_BYTES: u64 = 4_718_592;
pub const DEFAULT_JPEG_QUALITY: u8 = 80;

/// Resize overrides. Omitted fields use the defaults above.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageResizeOptions {
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub max_bytes: Option<u64>,
    pub jpeg_quality: Option<u8>,
}

/// A base64 image plus the dimensions needed for the coordinate hint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizedImage {
    pub data: String,
    pub mime_type: String,
    pub original_width: u32,
    pub original_height: u32,
    pub width: u32,
    pub height: u32,
    pub was_resized: bool,
}

#[derive(Debug, Clone, Copy)]
struct Limits {
    max_width: u32,
    max_height: u32,
    max_bytes: u64,
    jpeg_quality: u8,
}

impl Limits {
    fn from_options(options: Option<ImageResizeOptions>) -> Self {
        let options = options.unwrap_or_default();
        Self {
            max_width: options.max_width.unwrap_or(DEFAULT_MAX_WIDTH),
            max_height: options.max_height.unwrap_or(DEFAULT_MAX_HEIGHT),
            max_bytes: options.max_bytes.unwrap_or(DEFAULT_MAX_BYTES),
            jpeg_quality: options.jpeg_quality.unwrap_or(DEFAULT_JPEG_QUALITY),
        }
    }
}

struct Candidate {
    data: String,
    encoded_size: u64,
    mime_type: &'static str,
}

/// Resizes an image to fit the dimension and encoded-size limits.
///
/// Returns `None` when the image cannot be decoded or cannot be encoded below
/// `max_bytes`, matching the source's `null`.
pub fn resize_image_in_process(
    input_bytes: &[u8],
    mime_type: &str,
    options: Option<ImageResizeOptions>,
) -> Option<ResizedImage> {
    let limits = Limits::from_options(options);
    let input_base64_size = (input_bytes.len() as u64).div_ceil(3) * 4;

    let image = decode_image(input_bytes).ok()?;
    let image = apply_exif_orientation(image, input_bytes);
    let original_width = image.get_width();
    let original_height = image.get_height();

    if original_width <= limits.max_width
        && original_height <= limits.max_height
        && input_base64_size < limits.max_bytes
    {
        return Some(ResizedImage {
            data: encode_base64(input_bytes),
            mime_type: if mime_type.is_empty() {
                format!("image/{}", image_format(mime_type))
            } else {
                mime_type.to_owned()
            },
            original_width,
            original_height,
            width: original_width,
            height: original_height,
            was_resized: false,
        });
    }

    let (mut width, mut height) = target_dimensions(original_width, original_height, limits);
    let qualities = quality_steps(limits.jpeg_quality);

    loop {
        for candidate in encode_candidates(&image, width, height, &qualities)? {
            if candidate.encoded_size < limits.max_bytes {
                return Some(ResizedImage {
                    data: candidate.data,
                    mime_type: candidate.mime_type.to_owned(),
                    original_width,
                    original_height,
                    width,
                    height,
                    was_resized: true,
                });
            }
        }

        if width == 1 && height == 1 {
            break;
        }

        let next_width = next_dimension(width);
        let next_height = next_dimension(height);
        if next_width == width && next_height == height {
            break;
        }
        width = next_width;
        height = next_height;
    }

    None
}

fn target_dimensions(width: u32, height: u32, limits: Limits) -> (u32, u32) {
    let mut target_width = width;
    let mut target_height = height;

    if target_width > limits.max_width {
        target_height = scale(target_height, limits.max_width, target_width);
        target_width = limits.max_width;
    }
    if target_height > limits.max_height {
        target_width = scale(target_width, limits.max_height, target_height);
        target_height = limits.max_height;
    }

    (target_width, target_height)
}

/// `Math.round(value * factor / divisor)` for non-negative values.
fn scale(value: u32, factor: u32, divisor: u32) -> u32 {
    let scaled = f64::from(value) * f64::from(factor) / f64::from(divisor);
    (scaled + 0.5).floor() as u32
}

/// `Math.max(1, Math.floor(value * 0.75))`.
fn next_dimension(value: u32) -> u32 {
    if value == 1 {
        return 1;
    }
    ((f64::from(value) * 0.75).floor() as u32).max(1)
}

/// `Array.from(new Set([jpegQuality, 85, 70, 55, 40]))`.
fn quality_steps(jpeg_quality: u8) -> Vec<u8> {
    let mut steps = Vec::new();
    for quality in [jpeg_quality, 85, 70, 55, 40] {
        if !steps.contains(&quality) {
            steps.push(quality);
        }
    }
    steps
}

fn encode_candidates(image: &PhotonImage, width: u32, height: u32, qualities: &[u8]) -> Option<Vec<Candidate>> {
    let resized = photon_resize(image, width, height, RESIZE_FILTER);
    let mut candidates = Vec::with_capacity(qualities.len() + 1);
    candidates.push(candidate(png_bytes(&resized).ok()?, "image/png"));
    for &quality in qualities {
        candidates.push(candidate(jpeg_bytes(&resized, quality).ok()?, "image/jpeg"));
    }
    Some(candidates)
}

fn candidate(bytes: Vec<u8>, mime_type: &'static str) -> Candidate {
    let data = encode_base64(&bytes);
    Candidate {
        encoded_size: data.len() as u64,
        data,
        mime_type,
    }
}

/// `mimeType.split("/")[1] ?? "png"`.
fn image_format(mime_type: &str) -> &str {
    mime_type.split('/').nth(1).unwrap_or("png")
}
