//! Byte-for-byte differential test for the coding-agent image pipeline.
//!
//! Every operation calls the pinned TypeScript module through the oracle and
//! compares base64 payloads, dimensions, hints, and pixel buffers exactly.
//! Fixture images are generated here and sent to both implementations so both
//! sides decode identical bytes.

#[path = "support/oracle.rs"]
mod oracle;

#[path = "support/image_fixtures.rs"]
mod image_fixtures;

use std::io::Cursor;

use pi_ai::types::{ImageContent, TextContent, TextOrImageContent};
use pi_coding_agent::utils::exif_orientation::apply_exif_orientation;
use pi_coding_agent::utils::image_convert::{convert_image_bytes_to_png, convert_to_png};
use pi_coding_agent::utils::image_process::{ProcessImageOptions, ProcessImageResult, process_image};
use pi_coding_agent::utils::image_resize::{format_dimension_note, resize_image};
use pi_coding_agent::utils::image_resize_core::{ImageResizeOptions, ResizedImage, resize_image_in_process};
use pi_coding_agent::utils::photon::decode_image;
use pi_js_compat::base64::{buffer_from_base64, buffer_to_base64};
use serde_json::{Value, json};

const SOURCE_MODULE: &str = "packages/coding-agent/src/utils/photon.ts";

struct Cases {
    request_id: u64,
    operation: &'static str,
    cases: Vec<Value>,
}

#[test]
fn image_pipeline_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("coding-agent-image-pipeline-oracle.ts", SOURCE_MODULE);
    compare_convert_to_png(&mut oracle);
    compare_convert_bytes_to_png(&mut oracle);
    compare_resize_in_process(&mut oracle);
    compare_resize_in_worker(&mut oracle);
    compare_dimension_notes(&mut oracle);
    compare_process_image(&mut oracle);
}

#[test]
fn exif_orientation_matches_common_pin() {
    let mut oracle = oracle::TsOracle::spawn("coding-agent-image-pipeline-oracle.ts", SOURCE_MODULE);
    compare_exif_orientation(&mut oracle);
}

fn request(cases: &Cases) -> Value {
    json!({"id": cases.request_id, "operation": cases.operation, "cases": cases.cases})
}

fn compare_exif_orientation(oracle: &mut oracle::TsOracle) {
    let cases = orientation_fixtures()
        .into_iter()
        .map(|(name, bytes)| json!({"name": name, "bytesHex": hex(&bytes)}))
        .collect::<Vec<_>>();
    let cases = Cases {
        request_id: 27_100,
        operation: "exif_orientation",
        cases,
    };

    let expected = oracle.request(&request(&cases));
    let results = orientation_fixtures()
        .into_iter()
        .map(|(name, bytes)| {
            let image = decode_image(&bytes).expect("fixture must decode");
            let applied = apply_exif_orientation(image, &bytes);
            json!({
                "name": name,
                "width": applied.get_width(),
                "height": applied.get_height(),
                "pixelsHex": hex(&applied.get_raw_pixels()),
            })
        })
        .collect::<Vec<_>>();

    assert_matches("exif orientation", results, &expected);
}

fn compare_convert_to_png(oracle: &mut oracle::TsOracle) {
    let fixtures = [
        ("tiny-png", image_fixtures::TINY_PNG.to_owned(), "image/png"),
        ("tiny-jpeg", image_fixtures::TINY_JPEG.to_owned(), "image/jpeg"),
        (
            "large-png",
            buffer_to_base64(&image_fixtures::checker_png(64, 48)),
            "image/png",
        ),
        (
            "webp",
            buffer_to_base64(&encode(
                &image_fixtures::checker_png(24, 16),
                image::ImageOutputFormat::WebP,
            )),
            "image/webp",
        ),
        (
            "gif",
            buffer_to_base64(&encode(
                &image_fixtures::checker_png(16, 12),
                image::ImageOutputFormat::Gif,
            )),
            "image/gif",
        ),
        ("undecodable", "bm90LWFuLWltYWdl".to_owned(), "image/jpeg"),
        ("empty", String::new(), "image/gif"),
        (
            "png-mime-with-jpeg-bytes",
            image_fixtures::TINY_JPEG.to_owned(),
            "image/png",
        ),
    ];

    let cases = Cases {
        request_id: 27_101,
        operation: "convert_to_png",
        cases: fixtures
            .iter()
            .map(|(name, data, mime_type)| json!({"name": name, "base64": data, "mimeType": mime_type}))
            .collect(),
    };

    let expected = oracle.request(&request(&cases));
    let results = fixtures
        .iter()
        .map(|(name, data, mime_type)| match convert_to_png(data, mime_type) {
            Some(converted) => json!({"name": name, "data": converted.data, "mimeType": converted.mime_type}),
            None => json!({"name": name, "data": null, "mimeType": null}),
        })
        .collect::<Vec<_>>();

    assert_matches("convert to png", results, &expected);
}

fn compare_convert_bytes_to_png(oracle: &mut oracle::TsOracle) {
    let fixtures = conversion_fixtures();
    let cases = Cases {
        request_id: 27_102,
        operation: "convert_bytes_to_png",
        cases: fixtures
            .iter()
            .map(|(name, bytes)| json!({"name": name, "bytesHex": hex(bytes)}))
            .collect(),
    };

    let expected = oracle.request(&request(&cases));
    let results = fixtures
        .iter()
        .map(|(name, bytes)| {
            json!({
                "name": name,
                "data": convert_image_bytes_to_png(bytes).map(|png| buffer_to_base64(&png)),
            })
        })
        .collect::<Vec<_>>();

    assert_matches("convert bytes to png", results, &expected);
}

fn compare_resize_in_process(oracle: &mut oracle::TsOracle) {
    compare_resize(oracle, 27_103, "resize_in_process", false);
}

fn compare_resize_in_worker(oracle: &mut oracle::TsOracle) {
    compare_resize(oracle, 27_104, "resize_in_worker", true);
}

fn compare_resize(oracle: &mut oracle::TsOracle, request_id: u64, operation: &'static str, use_worker: bool) {
    let fixtures = resize_fixtures();
    let cases = Cases {
        request_id,
        operation,
        cases: fixtures
            .iter()
            .map(|fixture| {
                json!({
                    "name": fixture.name,
                    "bytesHex": hex(&fixture.bytes),
                    "mimeType": fixture.mime_type,
                    "options": fixture.options,
                })
            })
            .collect(),
    };

    let expected = oracle.request(&request(&cases));
    let results = fixtures
        .iter()
        .map(|fixture| {
            let options = resize_options(&fixture.options);
            let result = if use_worker {
                futures_lite_block_on(resize_image(&fixture.bytes, fixture.mime_type, options))
            } else {
                resize_image_in_process(&fixture.bytes, fixture.mime_type, options)
            };
            json!({"name": fixture.name, "result": result})
        })
        .collect::<Vec<_>>();

    assert_matches("resize via {operation}", results, &expected);
}

/// Runs one future to completion on a single-threaded runtime.
fn futures_lite_block_on(
    future: impl std::future::Future<Output = Result<Option<ResizedImage>, tokio::task::JoinError>>,
) -> Option<ResizedImage> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(future).expect("resize task")
}

fn compare_dimension_notes(oracle: &mut oracle::TsOracle) {
    let results = [
        ("not-resized", 100, 100, 100, 100, false),
        ("exact-tie", 9, 9, 8, 8, true),
        ("round-down", 1000, 300, 300, 90, true),
        ("two-times", 2000, 1000, 1000, 500, true),
        ("identity", 1, 1, 1, 1, true),
        ("wide", 2400, 4800, 2000, 4000, true),
        ("thirds", 7, 7, 3, 3, true),
        ("hundredths", 1, 1, 8, 8, true),
    ];
    let cases = Cases {
        request_id: 27_105,
        operation: "format_dimension_note",
        cases: results
            .iter()
            .map(|(name, original_width, original_height, width, height, was_resized)| {
                json!({
                    "name": name,
                    "result": {
                        "data": "",
                        "mimeType": "image/png",
                        "originalWidth": original_width,
                        "originalHeight": original_height,
                        "width": width,
                        "height": height,
                        "wasResized": was_resized,
                    },
                })
            })
            .collect(),
    };

    let expected = oracle.request(&request(&cases));
    let actual = results
        .iter()
        .map(|(name, original_width, original_height, width, height, was_resized)| {
            let result = ResizedImage {
                data: String::new(),
                mime_type: "image/png".to_owned(),
                original_width: *original_width,
                original_height: *original_height,
                width: *width,
                height: *height,
                was_resized: *was_resized,
            };
            json!({"name": name, "value": format_dimension_note(&result)})
        })
        .collect::<Vec<_>>();

    assert_matches("dimension notes", actual, &expected);
}

fn compare_process_image(oracle: &mut oracle::TsOracle) {
    let fixtures = process_fixtures();
    let cases = Cases {
        request_id: 27_106,
        operation: "process_image",
        cases: fixtures
            .iter()
            .map(|fixture| {
                json!({
                    "name": fixture.name,
                    "bytesHex": hex(&fixture.bytes),
                    "mimeType": fixture.mime_type,
                    "options": fixture.options,
                })
            })
            .collect(),
    };

    let expected = oracle.request(&request(&cases));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let results = fixtures
        .iter()
        .map(|fixture| {
            let options = process_options(&fixture.options);
            let result = runtime.block_on(process_image(&fixture.bytes, fixture.mime_type, options));
            json!({"name": fixture.name, "result": process_result(&result)})
        })
        .collect::<Vec<_>>();

    assert_matches("process image", results, &expected);
}

/// Compares per case so a failure names the fixture instead of dumping both payloads.
fn assert_matches(label: &str, results: Vec<Value>, expected: &Value) {
    let expected_results = expected["output"]["results"]
        .as_array()
        .unwrap_or_else(|| panic!("{label}: oracle returned {expected}"));
    assert_eq!(results.len(), expected_results.len(), "{label}: case count");
    for (actual, expected) in results.iter().zip(expected_results) {
        assert_eq!(redact(actual), redact(expected), "{label}: case {}", expected["name"]);
    }
}

/// Replaces long base64 payloads with their encoded length for readable diffs.
fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let value = match value {
                        Value::String(text) if key == "data" && text.len() > 32 => {
                            Value::String(format!("<{} base64 chars>", text.len()))
                        }
                        other => redact(other),
                    };
                    (key.clone(), value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact).collect()),
        other => other.clone(),
    }
}

fn process_result(result: &ProcessImageResult) -> Value {
    match result {
        ProcessImageResult::Processed { data, mime_type, hints } => {
            json!({"ok": true, "data": data, "mimeType": mime_type, "hints": hints})
        }
        ProcessImageResult::Omitted { message } => json!({"ok": false, "message": message}),
    }
}

fn resize_options(options: &Value) -> Option<ImageResizeOptions> {
    if options.is_null() {
        return None;
    }
    Some(ImageResizeOptions {
        max_width: options
            .get("maxWidth")
            .and_then(Value::as_u64)
            .map(|value| value as u32),
        max_height: options
            .get("maxHeight")
            .and_then(Value::as_u64)
            .map(|value| value as u32),
        max_bytes: options.get("maxBytes").and_then(Value::as_u64),
        jpeg_quality: options
            .get("jpegQuality")
            .and_then(Value::as_u64)
            .map(|value| value as u8),
    })
}

fn process_options(options: &Value) -> Option<ProcessImageOptions> {
    if options.is_null() {
        return None;
    }
    Some(ProcessImageOptions {
        auto_resize_images: options.get("autoResizeImages").and_then(Value::as_bool),
        resize_options: resize_options(options.get("resizeOptions").unwrap_or(&Value::Null)),
    })
}

struct Fixture {
    name: &'static str,
    bytes: Vec<u8>,
    mime_type: &'static str,
    options: Value,
}

fn orientation_fixtures() -> Vec<(&'static str, Vec<u8>)> {
    let mut fixtures = Vec::new();
    for orientation in 1..=8u16 {
        for little_endian in [true, false] {
            fixtures.push((
                if little_endian { "jpeg-le" } else { "jpeg-be" },
                with_exif(&image_fixtures::checker_png(3, 2), orientation, little_endian),
            ));
        }
        fixtures.push((
            "webp-exif",
            webp_with_orientation(orientation, image_fixtures::checker_png(2, 3)),
        ));
    }
    fixtures.push((
        "jpeg-without-exif",
        encode(&image_fixtures::checker_png(3, 2), image::ImageOutputFormat::Jpeg(90)),
    ));
    fixtures
}

fn conversion_fixtures() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("tiny-png", buffer_from_base64(image_fixtures::TINY_PNG)),
        ("tiny-jpeg", buffer_from_base64(image_fixtures::TINY_JPEG)),
        (
            "webp",
            encode(&image_fixtures::checker_png(20, 14), image::ImageOutputFormat::WebP),
        ),
        (
            "gif",
            encode(&image_fixtures::checker_png(12, 10), image::ImageOutputFormat::Gif),
        ),
        (
            "bmp",
            encode(&image_fixtures::checker_png(5, 4), image::ImageOutputFormat::Bmp),
        ),
        ("undecodable", b"not an image".to_vec()),
        ("empty", Vec::new()),
    ]
}

fn resize_fixtures() -> Vec<Fixture> {
    let png = |width, height| image_fixtures::checker_png(width, height);
    vec![
        Fixture {
            name: "within-limits",
            bytes: png(2, 2),
            mime_type: "image/png",
            options: json!({"maxWidth": 100, "maxHeight": 100, "maxBytes": 1_048_576}),
        },
        Fixture {
            name: "dimension-limit",
            bytes: png(100, 60),
            mime_type: "image/png",
            options: json!({"maxWidth": 50, "maxHeight": 50, "maxBytes": 1_048_576}),
        },
        Fixture {
            name: "height-limit",
            bytes: png(60, 100),
            mime_type: "image/png",
            options: json!({"maxWidth": 50, "maxHeight": 50, "maxBytes": 1_048_576}),
        },
        Fixture {
            name: "byte-limit",
            bytes: png(120, 90),
            mime_type: "image/png",
            options: json!({"maxWidth": 2000, "maxHeight": 2000, "maxBytes": 20_000}),
        },
        Fixture {
            name: "impossible-limit",
            bytes: png(40, 30),
            mime_type: "image/png",
            options: json!({"maxWidth": 2000, "maxHeight": 2000, "maxBytes": 1}),
        },
        Fixture {
            name: "jpeg-input",
            bytes: encode(&png(30, 20), image::ImageOutputFormat::Jpeg(90)),
            mime_type: "image/jpeg",
            options: json!({"maxWidth": 100, "maxHeight": 100, "maxBytes": 1_048_576}),
        },
        Fixture {
            name: "webp-input",
            bytes: encode(&png(30, 20), image::ImageOutputFormat::WebP),
            mime_type: "image/webp",
            options: json!({"maxWidth": 10, "maxHeight": 10, "maxBytes": 1_048_576}),
        },
        Fixture {
            name: "gif-input",
            bytes: encode(&png(24, 18), image::ImageOutputFormat::Gif),
            mime_type: "image/gif",
            options: json!({"maxWidth": 12, "maxHeight": 12, "maxBytes": 1_048_576}),
        },
        Fixture {
            name: "custom-quality",
            bytes: png(80, 60),
            mime_type: "image/png",
            options: json!({"maxWidth": 80, "maxHeight": 80, "maxBytes": 8_000, "jpegQuality": 30}),
        },
        Fixture {
            name: "high-quality",
            bytes: png(80, 60),
            mime_type: "image/png",
            options: json!({"maxWidth": 80, "maxHeight": 80, "maxBytes": 8_000, "jpegQuality": 95}),
        },
        Fixture {
            name: "degenerate-width",
            bytes: png(3, 2),
            mime_type: "image/png",
            options: json!({"maxWidth": 1, "maxHeight": 2000, "maxBytes": 1_048_576}),
        },
        Fixture {
            name: "degenerate-height",
            bytes: png(2, 3),
            mime_type: "image/png",
            options: json!({"maxWidth": 2000, "maxHeight": 1, "maxBytes": 1_048_576}),
        },
        Fixture {
            name: "defaults",
            bytes: png(120, 80),
            mime_type: "image/png",
            options: Value::Null,
        },
        Fixture {
            name: "empty-mime",
            bytes: png(4, 4),
            mime_type: "",
            options: json!({"maxWidth": 100, "maxHeight": 100, "maxBytes": 1_048_576}),
        },
        Fixture {
            name: "undecodable",
            bytes: b"not an image".to_vec(),
            mime_type: "image/png",
            options: Value::Null,
        },
    ]
}

fn process_fixtures() -> Vec<Fixture> {
    let png = |width, height| image_fixtures::checker_png(width, height);
    vec![
        Fixture {
            name: "bmp-without-resize",
            bytes: encode(&png(4, 3), image::ImageOutputFormat::Bmp),
            mime_type: "image/bmp",
            options: json!({"autoResizeImages": false}),
        },
        Fixture {
            name: "bmp-with-resize",
            bytes: encode(&png(60, 40), image::ImageOutputFormat::Bmp),
            mime_type: "image/bmp",
            options: Value::Null,
        },
        Fixture {
            name: "png-within-limits",
            bytes: png(20, 20),
            mime_type: "image/png",
            options: Value::Null,
        },
        Fixture {
            name: "png-resized",
            bytes: png(120, 80),
            mime_type: "image/png",
            options: json!({"resizeOptions": {"maxWidth": 40, "maxHeight": 40, "maxBytes": 1_048_576}}),
        },
        Fixture {
            name: "jpeg-alias",
            bytes: encode(&png(20, 20), image::ImageOutputFormat::Jpeg(90)),
            mime_type: "IMAGE/JPG; charset=binary",
            options: json!({"autoResizeImages": false}),
        },
        Fixture {
            name: "unsupported-mime-convertible",
            bytes: encode(&png(6, 5), image::ImageOutputFormat::Bmp),
            mime_type: "image/tiff",
            options: json!({"autoResizeImages": false}),
        },
        Fixture {
            name: "undecodable",
            bytes: b"not an image".to_vec(),
            mime_type: "image/tiff",
            options: Value::Null,
        },
        Fixture {
            name: "impossible-limit",
            bytes: png(40, 30),
            mime_type: "image/png",
            options: json!({"resizeOptions": {"maxWidth": 2000, "maxHeight": 2000, "maxBytes": 1}}),
        },
    ]
}

fn encode(bytes: &[u8], format: image::ImageOutputFormat) -> Vec<u8> {
    let image = image::load_from_memory(bytes).expect("fixture must decode");
    let mut encoded = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut encoded), format)
        .expect("fixture must encode");
    encoded
}

fn with_exif(jpeg: &[u8], orientation: u16, little_endian: bool) -> Vec<u8> {
    let jpeg = encode(jpeg, image::ImageOutputFormat::Jpeg(90));
    let mut tiff = Vec::new();
    let push16 = |tiff: &mut Vec<u8>, value: u16| {
        tiff.extend_from_slice(&if little_endian {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    };
    let push32 = |tiff: &mut Vec<u8>, value: u32| {
        tiff.extend_from_slice(&if little_endian {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    };
    tiff.extend_from_slice(if little_endian { b"II" } else { b"MM" });
    push16(&mut tiff, 42);
    push32(&mut tiff, 8);
    push16(&mut tiff, 1);
    push16(&mut tiff, 0x0112);
    push16(&mut tiff, 3);
    push32(&mut tiff, 1);
    push16(&mut tiff, orientation);
    push16(&mut tiff, 0);
    push32(&mut tiff, 0);

    let mut segment = vec![0xff, 0xe1];
    segment.extend_from_slice(&(2u16 + 6 + tiff.len() as u16).to_be_bytes());
    segment.extend_from_slice(b"Exif\0\0");
    segment.extend_from_slice(&tiff);

    let mut jpeg = jpeg;
    jpeg.splice(2..2, segment);
    jpeg
}

fn webp_with_orientation(orientation: u16, image_bytes: Vec<u8>) -> Vec<u8> {
    let tiff = {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x0112u16.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&0u16.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff
    };

    let mut webp = encode(&image_bytes, image::ImageOutputFormat::WebP);
    webp.extend_from_slice(b"EXIF");
    webp.extend_from_slice(&(tiff.len() as u32).to_le_bytes());
    webp.extend_from_slice(&tiff);
    if tiff.len() % 2 == 1 {
        webp.push(0);
    }
    let riff_size = (webp.len() - 8) as u32;
    webp[4..8].copy_from_slice(&riff_size.to_le_bytes());
    webp
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Asserts that text blocks keep their shape for the untagged content enum.
#[test]
fn text_blocks_serialize_with_the_wire_type_tag() {
    let block = TextOrImageContent::Text(TextContent::new("before"));
    assert_eq!(
        serde_json::to_value(&block).unwrap(),
        json!({"type": "text", "text": "before"})
    );
    let block = TextOrImageContent::Image(ImageContent::new("AA==", "image/png"));
    assert_eq!(
        serde_json::to_value(&block).unwrap(),
        json!({"type": "image", "data": "AA==", "mimeType": "image/png"})
    );
}
