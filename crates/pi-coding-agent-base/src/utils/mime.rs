use std::os::fd::IntoRawFd;
use std::os::unix::fs::FileExt;

use pi_js_compat::js_json::JsString;
use pi_js_compat::node_error::NativeNodeError;
use pi_js_compat::node_fs::{
    decode_node_path, node_file_read_error_record, node_fs_error_record, node_path_argument_error,
};

const IMAGE_TYPE_SNIFF_BYTES: usize = 4_100;
const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/// Detect a supported image MIME type from a byte prefix.
pub fn detect_supported_image_mime_type(buffer: &[u8]) -> Option<String> {
    if starts_with(buffer, &[0xff, 0xd8, 0xff]) {
        return (buffer.get(3) != Some(&0xf7)).then(|| "image/jpeg".to_owned());
    }
    if starts_with(buffer, &PNG_SIGNATURE) {
        return (is_png(buffer) && !is_animated_png(buffer)).then(|| "image/png".to_owned());
    }
    if starts_with_ascii(buffer, 0, "GIF") {
        return Some("image/gif".to_owned());
    }
    if starts_with_ascii(buffer, 0, "RIFF") && starts_with_ascii(buffer, 8, "WEBP") {
        return Some("image/webp".to_owned());
    }
    if starts_with_ascii(buffer, 0, "BM") && is_bmp(buffer) {
        return Some("image/bmp".to_owned());
    }
    None
}

/// Detect a supported image MIME type by reading up to 4100 bytes from a file.
pub async fn detect_supported_image_mime_type_from_file(
    file_path: &std::path::Path,
) -> Result<Option<String>, Box<NativeNodeError>> {
    let file_path = file_path.to_owned();
    match tokio::task::spawn_blocking(move || read_file_image_mime_type(&file_path)).await {
        Ok(result) => result,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

fn read_file_image_mime_type(file_path: &std::path::Path) -> Result<Option<String>, Box<NativeNodeError>> {
    let path = decode_node_path(file_path.as_os_str());
    if let Some(error) = node_path_argument_error(&path) {
        return Err(Box::new(error.into()));
    }
    let file = std::fs::File::open(file_path)
        .map_err(|error| Box::new(NativeNodeError::from(node_fs_error_record(&error, "open", &path))))?;
    let mut buffer = vec![0u8; IMAGE_TYPE_SNIFF_BYTES];
    let bytes_read = file
        .read_at(&mut buffer, 0)
        .map_err(|error| Box::new(NativeNodeError::from(node_file_read_error_record(&error))));
    nix::unistd::close(file.into_raw_fd())
        .map_err(std::io::Error::from)
        .map_err(|error| {
            Box::new(NativeNodeError::from(node_fs_error_record(
                &error,
                "close",
                &JsString::default(),
            )))
        })?;
    buffer.truncate(bytes_read?);
    Ok(detect_supported_image_mime_type(&buffer))
}

fn is_png(buffer: &[u8]) -> bool {
    buffer.len() >= 16 && read_uint32_be(buffer, PNG_SIGNATURE.len()) == 13 && starts_with_ascii(buffer, 12, "IHDR")
}

fn is_animated_png(buffer: &[u8]) -> bool {
    let mut offset = PNG_SIGNATURE.len();
    while offset + 8 <= buffer.len() {
        let chunk_length = read_uint32_be(buffer, offset);
        let chunk_type_offset = offset + 4;
        if starts_with_ascii(buffer, chunk_type_offset, "acTL") {
            return true;
        }
        if starts_with_ascii(buffer, chunk_type_offset, "IDAT") {
            return false;
        }

        let next_offset = offset + 8 + chunk_length as usize + 4;
        if next_offset <= offset || next_offset > buffer.len() {
            return false;
        }
        offset = next_offset;
    }
    false
}

fn is_bmp(buffer: &[u8]) -> bool {
    if buffer.len() < 26 {
        return false;
    }

    let declared_file_size = read_uint32_le(buffer, 2);
    let pixel_data_offset = read_uint32_le(buffer, 10);
    let dib_header_size = read_uint32_le(buffer, 14);
    if declared_file_size != 0 && declared_file_size < 26 {
        return false;
    }
    if pixel_data_offset < 14 + dib_header_size {
        return false;
    }
    if declared_file_size != 0 && pixel_data_offset >= declared_file_size {
        return false;
    }

    let (color_planes, bits_per_pixel) = if dib_header_size == 12 {
        (read_uint16_le(buffer, 22), read_uint16_le(buffer, 24))
    } else if (40..=124).contains(&dib_header_size) {
        if buffer.len() < 30 {
            return false;
        }
        (read_uint16_le(buffer, 26), read_uint16_le(buffer, 28))
    } else {
        return false;
    };

    color_planes == 1 && matches!(bits_per_pixel, 1 | 4 | 8 | 16 | 24 | 32)
}

fn read_uint16_le(buffer: &[u8], offset: usize) -> u16 {
    let low = u16::from(buffer.get(offset).copied().unwrap_or(0));
    let high = u16::from(buffer.get(offset + 1).copied().unwrap_or(0));
    low + (high << 8)
}

fn read_uint32_be(buffer: &[u8], offset: usize) -> u32 {
    let byte = |index: usize| u32::from(buffer.get(offset + index).copied().unwrap_or(0));
    (byte(0) << 24) + (byte(1) << 16) + (byte(2) << 8) + byte(3)
}

fn read_uint32_le(buffer: &[u8], offset: usize) -> u32 {
    let byte = |index: usize| u32::from(buffer.get(offset + index).copied().unwrap_or(0));
    byte(0) + (byte(1) << 8) + (byte(2) << 16) + (byte(3) << 24)
}

fn starts_with(buffer: &[u8], bytes: &[u8]) -> bool {
    buffer.get(..bytes.len()) == Some(bytes)
}

fn starts_with_ascii(buffer: &[u8], offset: usize, text: &str) -> bool {
    buffer
        .get(offset..offset + text.len())
        .is_some_and(|slice| slice == text.as_bytes())
}
