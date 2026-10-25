use pi_coding_agent::utils::image_process::{ProcessImageOptions, ProcessImageResult, process_image};
use pi_coding_agent::utils::image_resize_core::ImageResizeOptions;

use crate::shared_image_fixtures as image_fixtures;

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

fn expect_png_magic(data: &str) {
    let bytes = image_fixtures::decode(data);
    assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4e, 0x47]);
}

#[tokio::test]
async fn converts_bmp_to_png_when_auto_resize_is_disabled() {
    let result = process_image(
        &bmp_1x1_red_24bpp(),
        "image/bmp",
        Some(ProcessImageOptions {
            auto_resize_images: Some(false),
            resize_options: None,
        }),
    )
    .await;

    let ProcessImageResult::Processed { data, mime_type, hints } = result else {
        panic!("expected a processed image");
    };
    assert_eq!(mime_type, "image/png");
    assert_eq!(hints, vec!["[Image converted from image/bmp to image/png.]"]);
    expect_png_magic(&data);
}

#[tokio::test]
async fn converts_bmp_before_auto_resizing() {
    let result = process_image(&bmp_1x1_red_24bpp(), "image/bmp", None).await;

    let ProcessImageResult::Processed { data, mime_type, hints } = result else {
        panic!("expected a processed image");
    };
    assert_eq!(mime_type, "image/png");
    assert_eq!(hints, vec!["[Image converted from image/bmp to image/png.]"]);
    expect_png_magic(&data);
}

#[tokio::test]
async fn reports_omission_when_the_input_cannot_be_converted() {
    let result = process_image(b"not an image", "image/tiff", None).await;

    let ProcessImageResult::Omitted { message } = result else {
        panic!("expected an omission");
    };
    assert_eq!(
        message,
        "[Image omitted: could not be converted to a supported inline image format.]"
    );
}

#[tokio::test]
async fn reports_omission_when_the_image_cannot_fit_the_size_limit() {
    let result = process_image(
        &image_fixtures::decode(image_fixtures::LARGE_PNG_200X200),
        "image/png",
        Some(ProcessImageOptions {
            auto_resize_images: Some(true),
            resize_options: Some(ImageResizeOptions {
                max_width: Some(2000),
                max_height: Some(2000),
                max_bytes: Some(1),
                jpeg_quality: None,
            }),
        }),
    )
    .await;

    let ProcessImageResult::Omitted { message } = result else {
        panic!("expected an omission");
    };
    assert_eq!(
        message,
        "[Image omitted: could not be resized below the inline image size limit.]"
    );
}

#[tokio::test]
async fn normalizes_mime_aliases_and_parameters() {
    for mime_type in ["image/jpg", "IMAGE/JPEG; charset=binary", "image/jpeg;foo=bar"] {
        let result = process_image(
            &image_fixtures::decode(image_fixtures::TINY_JPEG),
            mime_type,
            Some(ProcessImageOptions {
                auto_resize_images: Some(false),
                resize_options: None,
            }),
        )
        .await;

        let ProcessImageResult::Processed { mime_type, hints, .. } = result else {
            panic!("expected a processed image for {mime_type}");
        };
        assert_eq!(mime_type, "image/jpeg", "{mime_type}");
        assert!(hints.is_empty(), "{mime_type}");
    }
}

#[tokio::test]
async fn adds_conversion_and_dimension_hints_when_resizing() {
    let png = image_fixtures::checker_png(2100, 2300);
    let result = process_image(&png, "image/png", None).await;

    let ProcessImageResult::Processed { hints, .. } = result else {
        panic!("expected a processed image");
    };
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(hints[0].contains("original 2100x2300"), "{hints:?}");
}

#[tokio::test]
async fn reports_both_hints_for_converted_and_resized_images() {
    let mut bmp = image_fixtures::checker_png(2100, 100);
    // Re-encode the oversized checker as BMP so the conversion and the resize
    // both contribute a hint.
    let decoded = image::load_from_memory(&bmp).unwrap().to_rgba8();
    bmp.clear();
    image::DynamicImage::ImageRgba8(decoded)
        .write_to(&mut std::io::Cursor::new(&mut bmp), image::ImageOutputFormat::Bmp)
        .unwrap();

    let result = process_image(&bmp, "image/bmp", None).await;

    let ProcessImageResult::Processed { hints, .. } = result else {
        panic!("expected a processed image");
    };
    assert_eq!(hints.len(), 2, "{hints:?}");
    assert_eq!(hints[0], "[Image converted from image/bmp to image/png.]");
    assert!(hints[1].contains("original 2100x100"), "{hints:?}");
}

#[tokio::test]
async fn keeps_already_supported_images_byte_identical_without_resizing() {
    let bytes = image_fixtures::decode(image_fixtures::TINY_PNG);
    let result = process_image(
        &bytes,
        "image/png",
        Some(ProcessImageOptions {
            auto_resize_images: Some(false),
            resize_options: None,
        }),
    )
    .await;

    let ProcessImageResult::Processed { data, mime_type, hints } = result else {
        panic!("expected a processed image");
    };
    assert_eq!(data, image_fixtures::TINY_PNG);
    assert_eq!(mime_type, "image/png");
    assert!(hints.is_empty());
}
