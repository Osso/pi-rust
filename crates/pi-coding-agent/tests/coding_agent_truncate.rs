use pi_coding_agent::core::tools::truncate::{
    GREP_MAX_LINE_LENGTH, TruncatedBy, TruncatedLine, TruncationOptions, TruncationResult, truncate_head,
    truncate_line, truncate_tail,
};
use pi_js_compat::js_json::JsString;

fn options(max_lines: usize, max_bytes: usize) -> TruncationOptions {
    TruncationOptions {
        max_lines: max_lines as f64,
        max_bytes: max_bytes as f64,
    }
}

#[test]
fn head_keeps_complete_leading_lines_within_the_byte_limit() {
    let result = truncate_head("aaa\nbbb\nccc\n", options(10, 7));

    assert_eq!(
        result,
        TruncationResult {
            content: JsString::from("aaa\nbbb"),
            truncated: true,
            truncated_by: Some(TruncatedBy::Bytes),
            total_lines: 3,
            total_bytes: 12,
            output_lines: 2,
            output_bytes: 7,
            last_line_partial: false,
            first_line_exceeds_limit: false,
            max_lines: 10_f64,
            max_bytes: 7_f64,
        }
    );
}

#[test]
fn head_reports_an_oversized_first_line_without_partial_output() {
    let result = truncate_head("ééé\nb", options(10, 5));

    assert_eq!(result.content, "");
    assert!(result.first_line_exceeds_limit);
    assert_eq!(result.truncated_by, Some(TruncatedBy::Bytes));
    assert_eq!(result.total_bytes, 8);
}

#[test]
fn tail_keeps_a_utf8_safe_suffix_of_an_oversized_last_line() {
    let result = truncate_tail("first\nxé😀", options(10, 5));

    assert_eq!(result.content, "😀");
    assert!(result.last_line_partial);
    assert_eq!(result.output_lines, 1);
    assert_eq!(result.output_bytes, 4);
}

#[test]
fn tail_classifies_the_line_limit() {
    let result = truncate_tail(
        "1\n2\n3\n4",
        TruncationOptions {
            max_lines: 2_f64,
            ..Default::default()
        },
    );

    assert_eq!(result.content, "3\n4");
    assert_eq!(result.truncated_by, Some(TruncatedBy::Lines));
    assert_eq!(result.total_lines, 4);
}

#[test]
fn untruncated_content_preserves_the_trailing_line_feed() {
    let result = truncate_head("a\nb\n", TruncationOptions::default());

    assert_eq!(result.content, "a\nb\n");
    assert!(!result.truncated);
    assert_eq!(result.output_lines, 2);
}

#[test]
fn line_truncation_appends_the_source_marker() {
    assert_eq!(
        truncate_line("abcdef", Some(3.0)),
        TruncatedLine {
            text: JsString::from("abc... [truncated]"),
            was_truncated: true
        }
    );
    assert_eq!(GREP_MAX_LINE_LENGTH, 500);
    assert!(!truncate_line("x".repeat(500), None).was_truncated);
}

#[test]
fn truncation_api_is_exported_from_the_tools_index_and_crate_root() {
    let tools =
        pi_coding_agent::core::tools::truncate_head("x", pi_coding_agent::core::tools::TruncationOptions::default());
    let root = pi_coding_agent::truncate_tail("x", pi_coding_agent::TruncationOptions::default());
    let line: pi_coding_agent::TruncationResult = root.clone();

    assert_eq!(tools, line);
    assert_eq!(pi_coding_agent::truncate_line("x", None).text, "x");
    assert_eq!(pi_coding_agent::core::tools::truncate_line("x", None).text, "x");
}
