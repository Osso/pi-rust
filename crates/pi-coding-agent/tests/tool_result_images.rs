use std::borrow::Cow;

use pi_ai::types::{ImageContent, TextContent};
use pi_coding_agent::utils::tool_result_images::{
    NormalizeToolResultImagesOptions, ToolResultContent, normalize_tool_result_images,
};
use pi_js_compat::base64::{buffer_from_base64, buffer_to_base64};

#[path = "support/image_fixtures.rs"]
mod image_fixtures;

fn text(value: &str) -> ToolResultContent {
    ToolResultContent::Text(TextContent::new(value))
}

fn image(bytes: &[u8], mime_type: &str) -> ToolResultContent {
    ToolResultContent::Image(ImageContent::new(buffer_to_base64(bytes), mime_type))
}

fn bmp_1x1_red_24bpp() -> Vec<u8> {
    let mut buffer = vec![0u8; 58];
    buffer[0..2].copy_from_slice(b"BM");
    buffer[2..6].copy_from_slice(&(58u32).to_le_bytes());
    buffer[10..14].copy_from_slice(&54u32.to_le_bytes());
    buffer[14..18].copy_from_slice(&40u32.to_le_bytes());
    buffer[18..22].copy_from_slice(&1i32.to_le_bytes());
    buffer[22..26].copy_from_slice(&1i32.to_le_bytes());
    buffer[26..28].copy_from_slice(&1u16.to_le_bytes());
    buffer[28..30].copy_from_slice(&24u16.to_le_bytes());
    buffer[34..38].copy_from_slice(&4u32.to_le_bytes());
    buffer[56] = 0xff;
    buffer
}

fn image_dimensions(base64: &str) -> (u32, u32) {
    let image = image::load_from_memory(&buffer_from_base64(base64)).expect("normalized image must decode");
    (image.width(), image.height())
}

#[tokio::test]
async fn returns_the_original_slice_when_there_are_no_image_blocks() {
    let content = vec![text("no images here")];
    let normalized = normalize_tool_result_images(&content, None).await;
    assert!(matches!(normalized, Cow::Borrowed(_)));
}

#[tokio::test]
async fn returns_the_original_slice_when_images_are_within_limits() {
    let content = vec![
        text("screenshot"),
        image(&image_fixtures::decode(image_fixtures::TINY_PNG_1X1), "image/png"),
    ];
    let normalized = normalize_tool_result_images(&content, None).await;
    assert!(matches!(normalized, Cow::Borrowed(_)));
}

#[tokio::test]
async fn resizes_oversized_images_and_reports_the_original_dimensions() {
    let content = vec![image(&image_fixtures::checker_png(2400, 4800), "image/png")];
    let normalized = normalize_tool_result_images(&content, None).await;

    assert!(matches!(normalized, Cow::Owned(_)));
    assert_eq!(normalized.len(), 2);
    let ToolResultContent::Image(image) = &normalized[0] else {
        panic!("expected an image block");
    };
    let (width, height) = image_dimensions(&image.data);
    assert!(width <= 2000 && height <= 2000);
    let ToolResultContent::Text(note) = &normalized[1] else {
        panic!("expected a text block");
    };
    assert!(note.text.contains("original 2400x4800"), "{}", note.text);
}

#[tokio::test]
async fn leaves_oversized_images_alone_when_auto_resize_is_disabled() {
    let content = vec![image(&image_fixtures::checker_png(2400, 4800), "image/png")];
    let options = NormalizeToolResultImagesOptions {
        auto_resize_images: Some(false),
    };
    let normalized = normalize_tool_result_images(&content, Some(options)).await;
    assert!(matches!(normalized, Cow::Borrowed(_)));
}

#[tokio::test]
async fn converts_unsupported_formats_even_when_auto_resize_is_disabled() {
    let content = vec![image(&bmp_1x1_red_24bpp(), "image/bmp")];
    let options = NormalizeToolResultImagesOptions {
        auto_resize_images: Some(false),
    };
    let normalized = normalize_tool_result_images(&content, Some(options)).await;

    assert!(matches!(normalized, Cow::Owned(_)));
    let ToolResultContent::Image(image) = &normalized[0] else {
        panic!("expected an image block");
    };
    assert_eq!(image.mime_type, "image/png");
    let ToolResultContent::Text(hint) = &normalized[1] else {
        panic!("expected a text block");
    };
    assert_eq!(hint.text, "[Image converted from image/bmp to image/png.]");
}

#[tokio::test]
async fn keeps_undecodable_images_instead_of_dropping_tool_output() {
    let content = vec![ToolResultContent::Image(ImageContent::new(
        "bm90LWFuLWltYWdl",
        "image/png",
    ))];
    let normalized = normalize_tool_result_images(&content, None).await;
    assert!(matches!(normalized, Cow::Borrowed(_)));
}

#[tokio::test]
async fn preserves_surrounding_text_blocks_and_their_order() {
    let content = vec![
        text("before"),
        image(&image_fixtures::checker_png(2400, 100), "image/png"),
        text("after"),
    ];
    let normalized = normalize_tool_result_images(&content, None).await;

    let kinds = normalized
        .iter()
        .map(|block| match block {
            ToolResultContent::Text(_) => "text",
            ToolResultContent::Image(_) => "image",
        })
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["text", "image", "text", "text"]);
    assert_eq!(normalized[0], text("before"));
    assert_eq!(normalized[3], text("after"));
}

#[tokio::test]
async fn keeps_already_normalized_blocks_when_nothing_changes() {
    // A supported, small image passes through unchanged even when the pipeline runs.
    let original = image(&image_fixtures::decode(image_fixtures::TINY_PNG_1X1), "image/png");
    let content = vec![original.clone()];
    let normalized = normalize_tool_result_images(&content, None).await;
    assert_eq!(normalized.as_ref(), [original]);
}
