//! Ports the `processFileArguments` cases from `test/block-images.test.ts` and
//! `test/image-resize-callers.test.ts`.

use pi_js_compat::js_json::JsString;
use std::path::PathBuf;
use std::time::SystemTime;

use pi_coding_agent::cli::file_processor::{ProcessFileOptions, process_file_arguments};

const TINY_PNG_BASE64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-file-processor-{name}-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tiny_bmp_1x1_red_24bpp() -> Vec<u8> {
    let mut buffer = vec![0_u8; 58];
    buffer[0..2].copy_from_slice(b"BM");
    buffer[2..6].copy_from_slice(&(58_u32).to_le_bytes());
    buffer[10..14].copy_from_slice(&54_u32.to_le_bytes());
    buffer[14..18].copy_from_slice(&40_u32.to_le_bytes());
    buffer[18..22].copy_from_slice(&1_i32.to_le_bytes());
    buffer[22..26].copy_from_slice(&1_i32.to_le_bytes());
    buffer[26..28].copy_from_slice(&1_u16.to_le_bytes());
    buffer[28..30].copy_from_slice(&24_u16.to_le_bytes());
    buffer[30..34].copy_from_slice(&0_u32.to_le_bytes());
    buffer[34..38].copy_from_slice(&4_u32.to_le_bytes());
    buffer[56] = 0xff;
    buffer
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn png_bytes() -> Vec<u8> {
    pi_js_compat::base64::buffer_from_base64(TINY_PNG_BASE64)
}

#[test]
fn always_processes_png_images() {
    runtime().block_on(async {
        let dir = temp_dir("png");
        let image_path = dir.join("test.png");
        std::fs::write(&image_path, png_bytes()).unwrap();

        let result = process_file_arguments(
            &[image_path.display().to_string()],
            None,
            &JsString::from(&dir.display().to_string()),
        )
        .await
        .unwrap();

        assert_eq!(result.images.len(), 1);
        assert_eq!(result.images[0].mime_type, "image/png");
        assert!(
            result
                .text
                .contains(format!("<file name=\"{}\">", image_path.display()))
        );
        std::fs::remove_dir_all(dir).unwrap();
    });
}

#[test]
fn processes_bmp_images_from_disk_as_png_attachments() {
    runtime().block_on(async {
        let dir = temp_dir("bmp");
        let image_path = dir.join("test.bmp");
        std::fs::write(&image_path, tiny_bmp_1x1_red_24bpp()).unwrap();

        let result = process_file_arguments(
            &[image_path.display().to_string()],
            None,
            &JsString::from(&dir.display().to_string()),
        )
        .await
        .unwrap();

        assert_eq!(result.images.len(), 1);
        assert_eq!(result.images[0].mime_type, "image/png");
        assert!(result.text.contains("[Image converted from image/bmp to image/png.]"));
        std::fs::remove_dir_all(dir).unwrap();
    });
}

#[test]
fn processes_text_files_normally() {
    runtime().block_on(async {
        let dir = temp_dir("text");
        let text_path = dir.join("test.txt");
        std::fs::write(&text_path, "Hello, world!").unwrap();

        let result = process_file_arguments(
            &[text_path.display().to_string()],
            None,
            &JsString::from(&dir.display().to_string()),
        )
        .await
        .unwrap();

        assert!(result.images.is_empty());
        assert_eq!(
            result.text,
            format!("<file name=\"{}\">\nHello, world!\n</file>\n", text_path.display())
        );
        std::fs::remove_dir_all(dir).unwrap();
    });
}

#[test]
fn skips_empty_files_and_keeps_argument_order() {
    runtime().block_on(async {
        let dir = temp_dir("order");
        let first = dir.join("first.txt");
        let empty = dir.join("empty.txt");
        let second = dir.join("second.txt");
        std::fs::write(&first, "one").unwrap();
        std::fs::write(&empty, "").unwrap();
        std::fs::write(&second, "two").unwrap();

        let result = process_file_arguments(
            &[
                first.display().to_string(),
                empty.display().to_string(),
                second.display().to_string(),
            ],
            None,
            &JsString::from(&dir.display().to_string()),
        )
        .await
        .unwrap();

        assert_eq!(
            result.text,
            format!(
                "<file name=\"{}\">\none\n</file>\n<file name=\"{}\">\ntwo\n</file>\n",
                first.display(),
                second.display()
            )
        );
        std::fs::remove_dir_all(dir).unwrap();
    });
}

#[test]
fn reports_missing_files_with_the_source_message() {
    runtime().block_on(async {
        let dir = temp_dir("missing");
        let missing = dir.join("missing.txt");

        let error = process_file_arguments(
            &[missing.display().to_string()],
            None,
            &JsString::from(&dir.display().to_string()),
        )
        .await
        .unwrap_err();

        assert_eq!(
            error.message(),
            &pi_js_compat::js_format!("File not found: {}", missing.display().to_string())
        );
        std::fs::remove_dir_all(dir).unwrap();
    });
}

#[test]
fn resolves_relative_paths_against_the_working_directory() {
    runtime().block_on(async {
        let dir = temp_dir("relative");
        std::fs::write(dir.join("relative.txt"), "content").unwrap();

        let result = process_file_arguments(
            &["relative.txt".to_owned()],
            Some(ProcessFileOptions {
                auto_resize_images: Some(false),
            }),
            &JsString::from(&dir.display().to_string()),
        )
        .await
        .unwrap();

        assert!(result.text.contains("content"));
        std::fs::remove_dir_all(dir).unwrap();
    });
}
