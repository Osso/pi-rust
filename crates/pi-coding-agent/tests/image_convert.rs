use crate::shared_image_fixtures as image_fixtures;

use pi_coding_agent::utils::image_convert::{convert_image_bytes_to_png, convert_to_png};

#[test]
fn returns_png_input_unchanged() {
    let converted = convert_to_png(image_fixtures::TINY_PNG, "image/png").expect("png passthrough");
    assert_eq!(converted.data, image_fixtures::TINY_PNG);
    assert_eq!(converted.mime_type, "image/png");
}

#[test]
fn converts_jpeg_to_png() {
    let converted = convert_to_png(image_fixtures::TINY_JPEG, "image/jpeg").expect("jpeg conversion");
    assert_eq!(converted.mime_type, "image/png");
    let bytes = image_fixtures::decode(&converted.data);
    assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4e, 0x47]);
}

#[test]
fn converts_raw_bytes_to_png() {
    let bytes = convert_image_bytes_to_png(&image_fixtures::decode(image_fixtures::TINY_JPEG)).expect("conversion");
    assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4e, 0x47]);
}

#[test]
fn returns_none_for_undecodable_input() {
    assert!(convert_image_bytes_to_png(b"not an image").is_none());
    assert!(convert_to_png("bm90LWFuLWltYWdl", "image/jpeg").is_none());
    assert!(convert_to_png("", "image/gif").is_none());
}

#[test]
fn applies_exif_orientation_before_encoding() {
    let mut jpeg = distinct_jpeg();
    // APP1 with a big-endian TIFF orientation entry (value 6).
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"MM");
    tiff.extend_from_slice(&42u16.to_be_bytes());
    tiff.extend_from_slice(&8u32.to_be_bytes());
    tiff.extend_from_slice(&1u16.to_be_bytes());
    tiff.extend_from_slice(&0x0112u16.to_be_bytes());
    tiff.extend_from_slice(&3u16.to_be_bytes());
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&6u16.to_be_bytes());
    tiff.extend_from_slice(&0u16.to_be_bytes());
    tiff.extend_from_slice(&0u32.to_be_bytes());

    let mut segment = vec![0xff, 0xe1];
    segment.extend_from_slice(&(2u16 + 6 + tiff.len() as u16).to_be_bytes());
    segment.extend_from_slice(b"Exif\0\0");
    segment.extend_from_slice(&tiff);
    jpeg.splice(2..2, segment);

    let converted = convert_to_png(&image_fixtures::encode(&jpeg), "image/jpeg").expect("conversion");
    let rotated = image::load_from_memory(&image_fixtures::decode(&converted.data)).unwrap();
    // Orientation 6 turns a 3x2 image into 2x3.
    assert_eq!((rotated.width(), rotated.height()), (2, 3));
}

/// 3x2 JPEG with distinct pixels so a quarter turn is observable.
fn distinct_jpeg() -> Vec<u8> {
    let image = image::RgbaImage::from_fn(3, 2, |x, y| image::Rgba([x as u8 * 40, y as u8 * 80, 7, 255]));
    let mut jpeg = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut jpeg),
            image::ImageOutputFormat::Jpeg(100),
        )
        .expect("fixture JPEG must encode");
    jpeg
}
