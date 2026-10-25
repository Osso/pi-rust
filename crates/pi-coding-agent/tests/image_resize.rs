use crate::shared_image_fixtures as image_fixtures;

use pi_coding_agent::utils::image_resize::{format_dimension_note, resize_image};
use pi_coding_agent::utils::image_resize_core::{ImageResizeOptions, resize_image_in_process};

fn png_dimensions(base64: &str) -> (u32, u32) {
    let bytes = image_fixtures::decode(base64);
    (
        u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
        u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
    )
}

#[tokio::test]
async fn keeps_caller_input_bytes_intact() {
    let input = image_fixtures::decode(image_fixtures::TINY_PNG);
    let original = input.clone();

    let result = resize_image(
        &input,
        "image/png",
        Some(ImageResizeOptions {
            max_width: Some(100),
            max_height: Some(100),
            max_bytes: Some(1024 * 1024),
            jpeg_quality: None,
        }),
    )
    .await
    .expect("resize must not fail")
    .expect("resize result");

    assert_eq!(input, original);
    assert_eq!(result.width, 2);
}

#[tokio::test]
async fn returns_the_original_image_within_all_limits() {
    let result = resize_image(
        &image_fixtures::decode(image_fixtures::TINY_PNG),
        "image/png",
        Some(ImageResizeOptions {
            max_width: Some(100),
            max_height: Some(100),
            max_bytes: Some(1024 * 1024),
            jpeg_quality: None,
        }),
    )
    .await
    .unwrap()
    .unwrap();

    assert!(!result.was_resized);
    assert_eq!(result.data, image_fixtures::TINY_PNG);
    assert_eq!(result.mime_type, "image/png");
    assert_eq!((result.original_width, result.original_height), (2, 2));
    assert_eq!((result.width, result.height), (2, 2));
}

#[tokio::test]
async fn resizes_images_exceeding_the_dimension_limits() {
    let result = resize_image(
        &image_fixtures::decode(image_fixtures::MEDIUM_PNG_100X100),
        "image/png",
        Some(ImageResizeOptions {
            max_width: Some(50),
            max_height: Some(50),
            max_bytes: Some(1024 * 1024),
            jpeg_quality: None,
        }),
    )
    .await
    .unwrap()
    .unwrap();

    assert!(result.was_resized);
    assert_eq!((result.original_width, result.original_height), (100, 100));
    assert!(result.width <= 50 && result.height <= 50);
    if result.mime_type == "image/png" {
        assert_eq!(png_dimensions(&result.data), (result.width, result.height));
    }
}

#[tokio::test]
async fn reduces_the_encoded_size_below_the_byte_limit() {
    let base64 = image_fixtures::LARGE_PNG_200X200;
    let original_size = image_fixtures::decode(base64).len();
    let max_bytes = (base64.len() as f64 * 0.9) as u64;

    let result = resize_image(
        &image_fixtures::decode(base64),
        "image/png",
        Some(ImageResizeOptions {
            max_width: Some(2000),
            max_height: Some(2000),
            max_bytes: Some(max_bytes),
            jpeg_quality: None,
        }),
    )
    .await
    .unwrap()
    .unwrap();

    assert!(result.was_resized);
    assert!(image_fixtures::decode(&result.data).len() < original_size);
    assert!(result.data.len() < base64.len());
    assert!(result.data.len() as u64 <= max_bytes);
}

#[tokio::test]
async fn returns_none_when_the_image_cannot_fit() {
    let result = resize_image(
        &image_fixtures::decode(image_fixtures::LARGE_PNG_200X200),
        "image/png",
        Some(ImageResizeOptions {
            max_width: Some(2000),
            max_height: Some(2000),
            max_bytes: Some(1),
            jpeg_quality: None,
        }),
    )
    .await
    .unwrap();

    assert!(result.is_none());
}

#[tokio::test]
async fn handles_jpeg_input_without_resizing() {
    let result = resize_image(
        &image_fixtures::decode(image_fixtures::TINY_JPEG),
        "image/jpeg",
        Some(ImageResizeOptions {
            max_width: Some(100),
            max_height: Some(100),
            max_bytes: Some(1024 * 1024),
            jpeg_quality: None,
        }),
    )
    .await
    .unwrap()
    .unwrap();

    assert!(!result.was_resized);
    assert_eq!((result.original_width, result.original_height), (2, 2));
    assert_eq!(result.mime_type, "image/jpeg");
}

#[tokio::test]
async fn returns_none_for_undecodable_bytes() {
    assert!(
        resize_image(b"not an image", "image/png", None)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn applies_the_default_limits() {
    // 2400x4800 exceeds the 2000px default on both axes.
    let png = image_fixtures::checker_png(2400, 4800);
    let result = resize_image(&png, "image/png", None).await.unwrap().unwrap();
    assert!(result.was_resized);
    assert!(result.width <= 2000 && result.height <= 2000);
    assert_eq!((result.original_width, result.original_height), (2400, 4800));
}

#[test]
fn picks_the_smallest_encoding_and_quality_step() {
    let bytes = image_fixtures::checker_png(1200, 900);
    let result = resize_image_in_process(
        &bytes,
        "image/png",
        Some(ImageResizeOptions {
            max_width: Some(400),
            max_height: Some(400),
            max_bytes: Some(20_000),
            jpeg_quality: None,
        }),
    )
    .expect("resize result");

    assert!(result.was_resized);
    assert!(result.data.len() <= 20_000);
    assert!(result.width <= 400 && result.height <= 400);
    assert!(["image/png", "image/jpeg"].contains(&result.mime_type.as_str()));
}

#[test]
fn falls_back_to_the_input_mime_type_for_an_empty_one() {
    let result = resize_image_in_process(
        &image_fixtures::decode(image_fixtures::TINY_PNG),
        "",
        Some(ImageResizeOptions {
            max_width: Some(100),
            max_height: Some(100),
            max_bytes: Some(1024 * 1024),
            jpeg_quality: None,
        }),
    )
    .unwrap();

    assert_eq!(result.mime_type, "image/png");
}

#[test]
fn keeps_the_declared_mime_type_verbatim() {
    let result = resize_image_in_process(
        &image_fixtures::decode(image_fixtures::TINY_PNG),
        "image/x-custom",
        None,
    )
    .unwrap();

    assert_eq!(result.mime_type, "image/x-custom");
}

#[test]
fn formats_dimension_notes_for_resized_images_only() {
    let result = resize_image_in_process(&image_fixtures::decode(image_fixtures::TINY_PNG), "image/png", None).unwrap();
    assert_eq!(format_dimension_note(&result), None);

    let resized = resize_image_in_process(
        &image_fixtures::decode(image_fixtures::MEDIUM_PNG_100X100),
        "image/png",
        Some(ImageResizeOptions {
            max_width: Some(50),
            max_height: Some(50),
            max_bytes: Some(1024 * 1024),
            jpeg_quality: None,
        }),
    )
    .unwrap();

    let note = format_dimension_note(&resized).expect("dimension note");
    assert!(note.contains("original 100x100"), "{note}");
    assert!(
        note.contains(&format!("displayed at {}x{}", resized.width, resized.height)),
        "{note}"
    );
    assert!(note.contains("Multiply coordinates by 2.00"), "{note}");
}

#[test]
fn rounds_dimension_scale_like_javascript_to_fixed() {
    // 9 / 8 = 1.125; ECMAScript toFixed(2) picks the larger of the two nearest
    // values ("1.13"), unlike Rust's half-to-even formatting ("1.12").
    let resized = resize_image_in_process(
        &image_fixtures::checker_png(9, 9),
        "image/png",
        Some(ImageResizeOptions {
            max_width: Some(8),
            max_height: Some(8),
            max_bytes: Some(1024 * 1024),
            jpeg_quality: None,
        }),
    )
    .unwrap();

    assert_eq!(resized.width, 8);
    let note = format_dimension_note(&resized).expect("dimension note");
    assert!(note.contains("Multiply coordinates by 1.13"), "{note}");
}
