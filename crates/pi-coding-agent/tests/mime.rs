use std::fs;

use pi_coding_agent::utils::mime::{detect_supported_image_mime_type, detect_supported_image_mime_type_from_file};
use tempfile::TempDir;

const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

fn png() -> Vec<u8> {
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 13]);
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&[0; 8]);
    bytes
}

fn animated_png() -> Vec<u8> {
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 8]);
    bytes.extend_from_slice(b"acTL");
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(&[0, 0, 0, 13]);
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&[0; 8]);
    bytes
}

fn bmp_with_header(header_size: u32, planes: u16, bits_per_pixel: u16) -> Vec<u8> {
    let mut bytes = vec![0u8; 40];
    bytes[0] = b'B';
    bytes[1] = b'M';
    // Zero declares an unknown file size, so header-size cases can use any offset.
    let file_size = 0u32;
    bytes[2..6].copy_from_slice(&file_size.to_le_bytes());
    let pixel_offset = 14 + header_size;
    bytes[10..14].copy_from_slice(&pixel_offset.to_le_bytes());
    bytes[14..18].copy_from_slice(&header_size.to_le_bytes());
    let planes_offset = if header_size == 12 { 22 } else { 26 };
    let bits_offset = planes_offset + 2;
    bytes[planes_offset..planes_offset + 2].copy_from_slice(&planes.to_le_bytes());
    bytes[bits_offset..bits_offset + 2].copy_from_slice(&bits_per_pixel.to_le_bytes());
    bytes
}

#[test]
fn detects_jpeg_and_rejects_lossless_jpeg() {
    assert_eq!(
        detect_supported_image_mime_type(&[0xff, 0xd8, 0xff, 0xe0, 0x00]),
        Some("image/jpeg".to_owned())
    );
    assert_eq!(detect_supported_image_mime_type(&[0xff, 0xd8, 0xff, 0xf7]), None);
    assert_eq!(detect_supported_image_mime_type(&[0xff, 0xd8]), None);
}

#[test]
fn detects_static_png_and_rejects_animated_png() {
    assert_eq!(detect_supported_image_mime_type(&png()), Some("image/png".to_owned()));
    assert_eq!(detect_supported_image_mime_type(&animated_png()), None);
}

#[test]
fn rejects_malformed_png_headers() {
    let mut wrong_length = PNG_SIGNATURE.to_vec();
    wrong_length.extend_from_slice(&[0, 0, 0, 12]);
    wrong_length.extend_from_slice(b"IHDR");
    wrong_length.extend_from_slice(&[0; 8]);
    assert_eq!(detect_supported_image_mime_type(&wrong_length), None);

    let mut wrong_type = PNG_SIGNATURE.to_vec();
    wrong_type.extend_from_slice(&[0, 0, 0, 13]);
    wrong_type.extend_from_slice(b"IDAT");
    wrong_type.extend_from_slice(&[0; 8]);
    assert_eq!(detect_supported_image_mime_type(&wrong_type), None);

    assert_eq!(detect_supported_image_mime_type(&PNG_SIGNATURE), None);
}

#[test]
fn detects_gif_and_webp() {
    assert_eq!(
        detect_supported_image_mime_type(b"GIF89a\x00\x00"),
        Some("image/gif".to_owned())
    );
    assert_eq!(
        detect_supported_image_mime_type(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
        Some("image/webp".to_owned())
    );
    assert_eq!(detect_supported_image_mime_type(b"RIFF\x00\x00\x00\x00WAVE"), None);
}

#[test]
fn detects_bmp_with_supported_headers() {
    for (header_size, bits) in [(12, 24), (40, 24), (124, 32), (40, 1), (40, 4), (40, 8), (40, 16)] {
        assert_eq!(
            detect_supported_image_mime_type(&bmp_with_header(header_size, 1, bits)),
            Some("image/bmp".to_owned()),
            "header {header_size} bits {bits}"
        );
    }
}

#[test]
fn rejects_invalid_bmp_headers() {
    assert_eq!(detect_supported_image_mime_type(&bmp_with_header(40, 2, 24)), None);
    assert_eq!(detect_supported_image_mime_type(&bmp_with_header(40, 1, 2)), None);
    assert_eq!(detect_supported_image_mime_type(&bmp_with_header(13, 1, 24)), None);
    assert_eq!(detect_supported_image_mime_type(&bmp_with_header(125, 1, 24)), None);

    let mut short = bmp_with_header(40, 1, 24);
    short.truncate(25);
    assert_eq!(detect_supported_image_mime_type(&short), None);

    let mut offset_before_header = bmp_with_header(40, 1, 24);
    offset_before_header[10..14].copy_from_slice(&10u32.to_le_bytes());
    assert_eq!(detect_supported_image_mime_type(&offset_before_header), None);

    let mut offset_at_end = bmp_with_header(40, 1, 24);
    let file_size = 60u32;
    offset_at_end[2..6].copy_from_slice(&file_size.to_le_bytes());
    offset_at_end[10..14].copy_from_slice(&file_size.to_le_bytes());
    assert_eq!(detect_supported_image_mime_type(&offset_at_end), None);

    let mut tiny_declared = bmp_with_header(40, 1, 24);
    tiny_declared[2..6].copy_from_slice(&25u32.to_le_bytes());
    assert_eq!(detect_supported_image_mime_type(&tiny_declared), None);
}

#[test]
fn rejects_unknown_formats() {
    assert_eq!(detect_supported_image_mime_type(b"not an image"), None);
    assert_eq!(detect_supported_image_mime_type(b""), None);
}

#[tokio::test]
async fn reads_image_mime_type_from_a_file() {
    let dir = TempDir::new().expect("temp dir");
    let png_path = dir.path().join("image.png");
    fs::write(&png_path, png()).expect("write png");
    assert_eq!(
        detect_supported_image_mime_type_from_file(&png_path)
            .await
            .expect("read png"),
        Some("image/png".to_owned())
    );

    let text_path = dir.path().join("notes.txt");
    fs::write(&text_path, "plain text").expect("write text");
    assert_eq!(
        detect_supported_image_mime_type_from_file(&text_path)
            .await
            .expect("read text"),
        None
    );

    assert!(
        detect_supported_image_mime_type_from_file(&dir.path().join("missing.png"))
            .await
            .is_err()
    );
}
