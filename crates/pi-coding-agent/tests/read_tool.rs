use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pi_agent_core::agent::{AgentTool, AgentToolResult};
use pi_ai::request::BoxFuture;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::tools::read::{ReadOperations, ReadTool, ReadToolOptions};
use pi_coding_agent::core::tools::truncate::DEFAULT_MAX_BYTES;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};

fn execute(tool: &ReadTool, params: JsValue) -> Result<AgentToolResult, JsString> {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime
        .block_on(tool.execute("call".to_owned(), params, None, Arc::new(|_result| {}), None))
        .map_err(|error| error.message)
}

fn text(result: &AgentToolResult) -> JsString {
    JsString::join(
        result.content.iter().filter_map(|entry| match entry {
            TextOrImageContent::Text(text) => Some(&text.text),
            TextOrImageContent::Image(_) => None,
        }),
        "\n",
    )
}

#[test]
fn reads_files_that_fit_within_limits() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("test.txt");
    let content = "Hello, world!\nLine 2\nLine 3";
    std::fs::write(&file, content).expect("fixture");
    let result = execute(
        &ReadTool::new(temp.path().to_string_lossy().as_ref()),
        json!({"path": file.to_string_lossy()}),
    )
    .expect("read result");
    assert_eq!(text(&result), content);
    assert!(!text(&result).contains("Use offset="));
    assert!(result.details.is_none());
}

#[test]
fn missing_files_report_enoent() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("nonexistent.txt");
    let error = execute(
        &ReadTool::new(temp.path().to_string_lossy().as_ref()),
        json!({"path": file.to_string_lossy()}),
    )
    .expect_err("missing file");
    assert!(error.contains("ENOENT"), "{error:?}");
    assert!(error.contains("access"), "{error:?}");
}

#[test]
fn truncates_files_exceeding_the_line_limit() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("large.txt");
    let lines = vec!["x"; 2500];
    std::fs::write(&file, lines.join("\n")).expect("fixture");
    let result = execute(
        &ReadTool::new(temp.path().to_string_lossy().as_ref()),
        json!({"path": file.to_string_lossy()}),
    )
    .expect("read result");
    let output = text(&result);
    assert_eq!(output.split('\n').filter(|line| *line == "x").count(), 2000);
    assert!(
        output.contains("[Showing lines 1-2000 of 2500. Use offset=2001 to continue.]"),
        "{output:?}"
    );
    let truncation = &result.details.expect("details")["truncation"];
    assert_eq!(truncation["truncated"], json!(true));
    assert_eq!(truncation["truncatedBy"], json!("lines"));
    assert_eq!(truncation["totalLines"], json!(2500));
    assert_eq!(truncation["outputLines"], json!(2000));
}

#[test]
fn truncates_when_the_byte_limit_is_exceeded() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("large-bytes.txt");
    let lines = (0..500)
        .map(|index| format!("Line {}: {}", index + 1, "x".repeat(200)))
        .collect::<Vec<_>>();
    std::fs::write(&file, lines.join("\n")).expect("fixture");
    let result = execute(
        &ReadTool::new(temp.path().to_string_lossy().as_ref()),
        json!({"path": file.to_string_lossy()}),
    )
    .expect("read result");
    let output = text(&result);
    assert!(output.contains("Line 1:"), "{output:?}");
    assert!(
        output.contains("[Showing lines 1-") && output.contains(" limit). Use offset="),
        "{output:?}"
    );
}

#[test]
fn caps_oversized_user_limits_before_returning_content() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("huge-user-limit.txt");
    let lines = (0..5000)
        .map(|index| format!("Line {}: {}", index + 1, "x".repeat(40)))
        .collect::<Vec<_>>();
    std::fs::write(&file, lines.join("\n")).expect("fixture");
    let result = execute(
        &ReadTool::new(temp.path().to_string_lossy().as_ref()),
        json!({"path": file.to_string_lossy(), "limit": 5000}),
    )
    .expect("read result");
    let output = text(&result);
    assert!(output.contains("Line 1:"), "{output:?}");
    assert!(!output.contains("Line 5000:"), "{output:?}");
    assert!(output.contains("Use offset="), "{output:?}");
    assert_eq!(result.details.expect("details")["truncation"]["truncated"], json!(true));
}

struct RangeOperations {
    full_reads: AtomicUsize,
    range_reads: AtomicUsize,
}

impl ReadOperations for RangeOperations {
    fn read_file<'a>(&'a self, _absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<Vec<u8>>> {
        self.full_reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(std::io::Error::other("full read should not be used")) })
    }

    fn supports_read_range(&self) -> bool {
        true
    }

    fn read_range<'a>(
        &'a self,
        _absolute_path: &'a JsString,
        start: u64,
        end: u64,
    ) -> BoxFuture<'a, std::io::Result<Vec<u8>>> {
        self.range_reads.fetch_add(1, Ordering::SeqCst);
        assert_eq!((start, end), (0, DEFAULT_MAX_BYTES as u64));
        Box::pin(async { Ok(b"Line 1\nLine 2\nLine 3".to_vec()) })
    }

    fn stat<'a>(&'a self, _absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<Option<u64>>> {
        Box::pin(async { Ok(Some(DEFAULT_MAX_BYTES as u64 * 4)) })
    }

    fn access<'a>(&'a self, _absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<()>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn avoids_full_reads_for_oversized_files() {
    let temp = tempfile::tempdir().expect("temp dir");
    let operations = Arc::new(RangeOperations {
        full_reads: AtomicUsize::new(0),
        range_reads: AtomicUsize::new(0),
    });
    let tool = ReadTool::with_options(
        temp.path().to_string_lossy().as_ref(),
        ReadToolOptions {
            auto_resize_images: None,
            operations: Some(operations.clone()),
        },
    );
    let result = execute(&tool, json!({"path": "large.txt"})).expect("read result");
    let output = text(&result);
    assert_eq!(operations.full_reads.load(Ordering::SeqCst), 0);
    assert_eq!(operations.range_reads.load(Ordering::SeqCst), 1);
    assert!(output.contains("Line 1"), "{output:?}");
    assert!(output.contains("Use offset="), "{output:?}");
}

#[test]
fn offset_and_limit_select_lines_and_report_remaining() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("offset-test.txt");
    let lines = (0..100).map(|index| format!("Line {}", index + 1)).collect::<Vec<_>>();
    std::fs::write(&file, lines.join("\n")).expect("fixture");
    let tool = ReadTool::new(temp.path().to_string_lossy().as_ref());

    let result = execute(&tool, json!({"path": file.to_string_lossy(), "offset": 51})).expect("offset read");
    let output = text(&result);
    assert!(!output.contains("Line 50"), "{output:?}");
    assert!(output.contains("Line 51"), "{output:?}");
    assert!(output.contains("Line 100"), "{output:?}");
    assert!(!output.contains("Use offset="), "{output:?}");

    let result = execute(&tool, json!({"path": file.to_string_lossy(), "limit": 10})).expect("limit read");
    let output = text(&result);
    assert!(output.contains("Line 1"), "{output:?}");
    assert!(output.contains("Line 10"), "{output:?}");
    assert!(!output.contains("Line 11"), "{output:?}");
    assert!(
        output.contains("[90 more lines in file. Use offset=11 to continue.]"),
        "{output:?}"
    );

    let result = execute(
        &tool,
        json!({"path": file.to_string_lossy(), "offset": 41, "limit": 20}),
    )
    .expect("offset+limit read");
    let output = text(&result);
    assert!(!output.contains("Line 40"), "{output:?}");
    assert!(output.contains("Line 41"), "{output:?}");
    assert!(output.contains("Line 60"), "{output:?}");
    assert!(!output.contains("Line 61"), "{output:?}");
    assert!(
        output.contains("[40 more lines in file. Use offset=61 to continue.]"),
        "{output:?}"
    );
}

#[test]
fn offset_beyond_end_reports_the_total_line_count() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("short.txt");
    std::fs::write(&file, "Line 1\nLine 2\nLine 3").expect("fixture");
    let error = execute(
        &ReadTool::new(temp.path().to_string_lossy().as_ref()),
        json!({"path": file.to_string_lossy(), "offset": 100}),
    )
    .expect_err("offset beyond end");
    assert_eq!(error, "Offset 100 is beyond end of file (3 lines total)");
}

#[test]
fn detects_images_from_file_magic() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("image.txt");
    let png = base64_png();
    std::fs::write(&file, &png).expect("fixture");
    let result = execute(
        &ReadTool::new(temp.path().to_string_lossy().as_ref()),
        json!({"path": file.to_string_lossy()}),
    )
    .expect("read result");
    assert!(
        text(&result).contains("Read image file [image/png]"),
        "{:?}",
        text(&result)
    );
    let image = result
        .content
        .iter()
        .find_map(|entry| match entry {
            TextOrImageContent::Image(image) => Some(image),
            TextOrImageContent::Text(_) => None,
        })
        .expect("image block");
    assert_eq!(image.mime_type, "image/png");
    assert!(!image.data.is_empty());
}

#[test]
fn converts_bmp_files_to_png_attachments() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("image.bmp");
    std::fs::write(&file, tiny_bmp()).expect("fixture");
    let result = execute(
        &ReadTool::new(temp.path().to_string_lossy().as_ref()),
        json!({"path": file.to_string_lossy()}),
    )
    .expect("read result");
    let output = text(&result);
    assert!(output.contains("Read image file [image/png]"), "{output:?}");
    assert!(
        output.contains("[Image converted from image/bmp to image/png.]"),
        "{output:?}"
    );
    let image = result
        .content
        .iter()
        .find_map(|entry| match entry {
            TextOrImageContent::Image(image) => Some(image),
            TextOrImageContent::Text(_) => None,
        })
        .expect("image block");
    assert_eq!(image.mime_type, "image/png");
}

#[test]
fn treats_image_extension_with_non_image_content_as_text() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("not-an-image.png");
    std::fs::write(&file, "definitely not a png").expect("fixture");
    let result = execute(
        &ReadTool::new(temp.path().to_string_lossy().as_ref()),
        json!({"path": file.to_string_lossy()}),
    )
    .expect("read result");
    assert!(text(&result).contains("definitely not a png"));
    assert!(
        !result
            .content
            .iter()
            .any(|entry| matches!(entry, TextOrImageContent::Image(_)))
    );
}

#[test]
fn aborted_reads_return_the_source_error() {
    let temp = tempfile::tempdir().expect("temp dir");
    let token = tokio_util::sync::CancellationToken::new();
    token.cancel();
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let error = runtime
        .block_on(ReadTool::new(temp.path().to_string_lossy().as_ref()).execute(
            "call".to_owned(),
            json!({"path": "file.txt"}),
            Some(token),
            Arc::new(|_result| {}),
            None,
        ))
        .expect_err("aborted");
    assert_eq!(error.message, "Operation aborted");
}

fn base64_png() -> Vec<u8> {
    const PNG: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAX+XDSwAAAABJRU5ErkJggg==";
    use pi_js_compat::base64::buffer_from_base64;
    buffer_from_base64(PNG)
}

fn tiny_bmp() -> Vec<u8> {
    let mut buffer = vec![0_u8; 58];
    buffer[0..2].copy_from_slice(b"BM");
    buffer[2..6].copy_from_slice(&58_u32.to_le_bytes());
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
