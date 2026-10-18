const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
/// Detects the supported image MIME type from a source-compatible byte prefix.
///
/// This performs format recognition only; it does not decode or validate image data.
pub fn detect_supported_image_mime_type(bytes: &[u8]) -> Option<&'static str> {
    if starts_with(bytes, &[0xff, 0xd8, 0xff]) {
        return (bytes.get(3) != Some(&0xf7)).then_some("image/jpeg");
    }
    if starts_with(bytes, &PNG_SIGNATURE) {
        return (is_png(bytes) && !is_animated_png(bytes)).then_some("image/png");
    }
    if starts_with_ascii(bytes, 0, "GIF") {
        return Some("image/gif");
    }
    if starts_with_ascii(bytes, 0, "RIFF") && starts_with_ascii(bytes, 8, "WEBP") {
        return Some("image/webp");
    }
    if starts_with_ascii(bytes, 0, "BM") && is_bmp(bytes) {
        return Some("image/bmp");
    }
    None
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.len() >= 16 && read_u32_be(bytes, PNG_SIGNATURE.len()) == 13 && starts_with_ascii(bytes, 12, "IHDR")
}

fn is_animated_png(bytes: &[u8]) -> bool {
    let mut offset = PNG_SIGNATURE.len();
    while offset
        .checked_add(8)
        .is_some_and(|header_end| header_end <= bytes.len())
    {
        let chunk_length = read_u32_be(bytes, offset);
        let chunk_type_offset = offset + 4;
        if starts_with_ascii(bytes, chunk_type_offset, "acTL") {
            return true;
        }
        if starts_with_ascii(bytes, chunk_type_offset, "IDAT") {
            return false;
        }
        let Some(next_offset) = offset
            .checked_add(8)
            .and_then(|value| value.checked_add(chunk_length as usize))
            .and_then(|value| value.checked_add(4))
        else {
            return false;
        };
        if next_offset <= offset || next_offset > bytes.len() {
            return false;
        }
        offset = next_offset;
    }
    false
}

fn is_bmp(bytes: &[u8]) -> bool {
    if bytes.len() < 26 {
        return false;
    }

    let declared_file_size = read_u32_le(bytes, 2);
    let pixel_data_offset = read_u32_le(bytes, 10);
    let dib_header_size = read_u32_le(bytes, 14);
    if declared_file_size != 0 && declared_file_size < 26 {
        return false;
    }
    let Some(minimum_pixel_offset) = 14u32.checked_add(dib_header_size) else {
        return false;
    };
    if pixel_data_offset < minimum_pixel_offset {
        return false;
    }
    if declared_file_size != 0 && pixel_data_offset >= declared_file_size {
        return false;
    }

    let (color_planes, bits_per_pixel) = match dib_header_size {
        12 => (read_u16_le(bytes, 22), read_u16_le(bytes, 24)),
        40..=124 if bytes.len() >= 30 => (read_u16_le(bytes, 26), read_u16_le(bytes, 28)),
        _ => return false,
    };
    color_planes == 1 && matches!(bits_per_pixel, 1 | 4 | 8 | 16 | 24 | 32)
}

fn read_u16_le(bytes: &[u8], offset: usize) -> u16 {
    let low = byte_at(bytes, offset);
    let high = byte_at(bytes, offset.saturating_add(1));
    u16::from(low) | (u16::from(high) << 8)
}

fn read_u32_be(bytes: &[u8], offset: usize) -> u32 {
    let first = byte_at(bytes, offset);
    let second = byte_at(bytes, offset.saturating_add(1));
    let third = byte_at(bytes, offset.saturating_add(2));
    let fourth = byte_at(bytes, offset.saturating_add(3));
    u32::from(first) << 24 | u32::from(second) << 16 | u32::from(third) << 8 | u32::from(fourth)
}

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    let first = byte_at(bytes, offset);
    let second = byte_at(bytes, offset.saturating_add(1));
    let third = byte_at(bytes, offset.saturating_add(2));
    let fourth = byte_at(bytes, offset.saturating_add(3));
    u32::from(first) | u32::from(second) << 8 | u32::from(third) << 16 | u32::from(fourth) << 24
}

fn byte_at(bytes: &[u8], offset: usize) -> u8 {
    bytes.get(offset).copied().unwrap_or(0)
}

fn starts_with(bytes: &[u8], prefix: &[u8]) -> bool {
    bytes.len() >= prefix.len() && bytes.starts_with(prefix)
}

fn starts_with_ascii(bytes: &[u8], offset: usize, text: &str) -> bool {
    let text_bytes = text.as_bytes();
    let Some(end) = offset.checked_add(text_bytes.len()) else {
        return false;
    };
    end <= bytes.len() && bytes[offset..end] == *text_bytes
}
