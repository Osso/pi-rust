//! EXIF orientation reading and application.
//!
//! Byte access mirrors JavaScript `Uint8Array` indexing: out-of-range and
//! negative indices read as `undefined`, which bitwise operators coerce to 0.
//!
//! Frozen `4f9e6f8a4` includes unsigned TIFF/chunk reads and bounded JPEG/WebP
//! scans. The native parser preserves those checks.

use photon_rs::PhotonImage;
use photon_rs::transform::{fliph, flipv};

const ORIENTATION_TAG: i64 = 0x0112;
const JPEG_APP1: u32 = 0xe1;

/// Reads the EXIF orientation (1-8) from JPEG or WebP bytes; defaults to 1.
pub fn read_exif_orientation(bytes: &[u8]) -> u8 {
    let tiff_offset = if is_jpeg(bytes) {
        find_jpeg_tiff_offset(bytes)
    } else if is_webp(bytes) {
        find_webp_tiff_offset(bytes)
    } else {
        None
    };

    tiff_offset.map_or(1, |offset| read_orientation_from_tiff(bytes, offset))
}

/// Applies the EXIF orientation of `original_bytes` to `image`.
///
/// Rotations return a new image with swapped dimensions; flips mutate in place.
pub fn apply_exif_orientation(image: PhotonImage, original_bytes: &[u8]) -> PhotonImage {
    match read_exif_orientation(original_bytes) {
        2 => {
            let mut image = image;
            fliph(&mut image);
            image
        }
        3 => {
            let mut image = image;
            fliph(&mut image);
            flipv(&mut image);
            image
        }
        4 => {
            let mut image = image;
            flipv(&mut image);
            image
        }
        5 => {
            let mut rotated = rotate_quarter_turn(&image, |x, y, _width, height| x * height + (height - 1 - y));
            fliph(&mut rotated);
            rotated
        }
        6 => rotate_quarter_turn(&image, |x, y, _width, height| x * height + (height - 1 - y)),
        7 => {
            let mut rotated = rotate_quarter_turn(&image, |x, y, width, height| (width - 1 - x) * height + y);
            fliph(&mut rotated);
            rotated
        }
        8 => rotate_quarter_turn(&image, |x, y, width, height| (width - 1 - x) * height + y),
        _ => image,
    }
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] == 0xd8
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() >= 12
        && &bytes[0..4] == b"RIFF"
        && bytes[8] == b'W'
        && bytes[9] == b'E'
        && bytes[10] == b'B'
        && bytes[11] == b'P'
}

fn read_orientation_from_tiff(bytes: &[u8], tiff_start: i64) -> u8 {
    let length = bytes.len() as i64;
    if tiff_start + 8 > length {
        return 1;
    }

    let byte_order = (byte_at(bytes, tiff_start) << 8) | byte_at(bytes, tiff_start + 1);
    let little_endian = byte_order == 0x4949;

    let ifd_start = tiff_start + read32(bytes, tiff_start + 4, little_endian);
    if ifd_start + 2 > length {
        return 1;
    }

    let entry_count = read16(bytes, ifd_start, little_endian);
    for entry in 0..entry_count {
        let entry_position = ifd_start + 2 + entry * 12;
        if entry_position + 12 > length {
            return 1;
        }
        if read16(bytes, entry_position, little_endian) == ORIENTATION_TAG {
            let value = read16(bytes, entry_position + 8, little_endian);
            return if (1..=8).contains(&value) { value as u8 } else { 1 };
        }
    }

    1
}

/// One step of the JPEG marker walk.
enum MarkerStep {
    /// Fill byte (`0xFF` marker prefix); advance by one.
    Fill,
    /// APP1 segment: the TIFF offset, or `None` when the segment is unusable.
    App1(Option<i64>),
    /// Ordinary segment; continue at this offset.
    Next(i64),
    /// Not a valid segment chain.
    Invalid,
}

fn find_jpeg_tiff_offset(bytes: &[u8]) -> Option<i64> {
    let length = bytes.len() as i64;
    let mut offset: i64 = 2;

    while offset < length - 1 {
        match jpeg_marker_step(bytes, offset) {
            MarkerStep::Fill => offset += 1,
            MarkerStep::App1(tiff_offset) => return tiff_offset,
            MarkerStep::Next(next_offset) => offset = next_offset,
            MarkerStep::Invalid => return None,
        }
    }

    None
}

fn jpeg_marker_step(bytes: &[u8], offset: i64) -> MarkerStep {
    let length = bytes.len() as i64;
    if byte_at(bytes, offset) != 0xff {
        return MarkerStep::Invalid;
    }

    let marker = byte_at(bytes, offset + 1);
    if marker == 0xff {
        return MarkerStep::Fill;
    }
    if marker == JPEG_APP1 {
        return MarkerStep::App1(jpeg_app1_tiff_offset(bytes, offset));
    }

    if offset + 4 > length {
        return MarkerStep::Invalid;
    }
    let segment_length = i64::from((byte_at(bytes, offset + 2) << 8) | byte_at(bytes, offset + 3));
    let next_offset = offset + 2 + segment_length;
    if next_offset > length || next_offset <= offset {
        return MarkerStep::Invalid;
    }
    MarkerStep::Next(next_offset)
}

fn jpeg_app1_tiff_offset(bytes: &[u8], offset: i64) -> Option<i64> {
    let length = bytes.len() as i64;
    if offset + 4 >= length {
        return None;
    }
    let segment_start = offset + 4;
    if segment_start + 6 > length || !has_exif_header(bytes, segment_start) {
        return None;
    }
    Some(segment_start + 6)
}

fn find_webp_tiff_offset(bytes: &[u8]) -> Option<i64> {
    let length = bytes.len() as i64;
    let mut offset: i64 = 12;

    while offset + 8 <= length {
        let chunk_id = [
            byte_at(bytes, offset) as u8,
            byte_at(bytes, offset + 1) as u8,
            byte_at(bytes, offset + 2) as u8,
            byte_at(bytes, offset + 3) as u8,
        ];
        let chunk_size = i64::from(
            byte_at(bytes, offset + 4)
                | byte_at(bytes, offset + 5) << 8
                | byte_at(bytes, offset + 6) << 16
                | byte_at(bytes, offset + 7) << 24,
        );
        let data_start = offset + 8;
        // RIFF chunks are padded to an even size.
        let next_offset = data_start + chunk_size + chunk_size % 2;
        if next_offset > length || next_offset <= offset {
            return None;
        }

        if &chunk_id == b"EXIF" {
            let tiff_start = if chunk_size >= 6 && has_exif_header(bytes, data_start) {
                data_start + 6
            } else {
                data_start
            };
            return Some(tiff_start);
        }

        offset = next_offset;
    }

    None
}

fn has_exif_header(bytes: &[u8], offset: i64) -> bool {
    (0..6).all(|index| byte_at(bytes, offset + index) == u32::from(b"Exif\0\0"[index as usize]))
}

fn byte_at(bytes: &[u8], index: i64) -> u32 {
    if index < 0 {
        return 0;
    }
    bytes.get(index as usize).copied().map_or(0, u32::from)
}

fn read16(bytes: &[u8], position: i64, little_endian: bool) -> i64 {
    if little_endian {
        i64::from(byte_at(bytes, position) | (byte_at(bytes, position + 1) << 8))
    } else {
        i64::from((byte_at(bytes, position) << 8) | byte_at(bytes, position + 1))
    }
}

fn read32(bytes: &[u8], position: i64, little_endian: bool) -> i64 {
    if little_endian {
        i64::from(
            byte_at(bytes, position)
                | byte_at(bytes, position + 1) << 8
                | byte_at(bytes, position + 2) << 16
                | byte_at(bytes, position + 3) << 24,
        )
    } else {
        i64::from(
            byte_at(bytes, position) << 24
                | byte_at(bytes, position + 1) << 16
                | byte_at(bytes, position + 2) << 8
                | byte_at(bytes, position + 3),
        )
    }
}

fn rotate_quarter_turn(image: &PhotonImage, destination_index: impl Fn(u32, u32, u32, u32) -> u32) -> PhotonImage {
    let width = image.get_width();
    let height = image.get_height();
    let source = image.get_raw_pixels();
    let mut destination = vec![0u8; source.len()];

    for y in 0..height {
        for x in 0..width {
            let source_index = ((y * width + x) * 4) as usize;
            let destination_index = (destination_index(x, y, width, height) * 4) as usize;
            destination[destination_index..destination_index + 4]
                .copy_from_slice(&source[source_index..source_index + 4]);
        }
    }

    PhotonImage::new(destination, height, width)
}
