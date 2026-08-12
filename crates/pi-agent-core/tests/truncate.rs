use pi_agent_core::harness::utils::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH, TruncatedBy, TruncatedLine, TruncationOptions,
    TruncationResult, format_size, truncate_head, truncate_line, truncate_tail,
};

const TRUNCATION_MARKER: &str = "... [truncated]";

fn options(max_lines: usize, max_bytes: usize) -> TruncationOptions {
    TruncationOptions { max_lines, max_bytes }
}

fn result(
    content: &str,
    truncated: bool,
    truncated_by: Option<TruncatedBy>,
    total_lines: usize,
    total_bytes: usize,
    output_lines: usize,
    output_bytes: usize,
    last_line_partial: bool,
    first_line_exceeds_limit: bool,
    max_lines: usize,
    max_bytes: usize,
) -> TruncationResult {
    TruncationResult {
        content: content.to_owned(),
        truncated,
        truncated_by,
        total_lines,
        total_bytes,
        output_lines,
        output_bytes,
        last_line_partial,
        first_line_exceeds_limit,
        max_lines,
        max_bytes,
    }
}

#[test]
fn constants_and_default_options_match_source() {
    assert_eq!(DEFAULT_MAX_LINES, 2_000);
    assert_eq!(DEFAULT_MAX_BYTES, 10 * 1024);
    assert_eq!(GREP_MAX_LINE_LENGTH, 500);

    let defaults = TruncationOptions::default();
    assert_eq!(defaults.max_lines, DEFAULT_MAX_LINES);
    assert_eq!(defaults.max_bytes, DEFAULT_MAX_BYTES);
}

#[test]
fn format_size_matches_byte_kilobyte_and_megabyte_thresholds() {
    assert_eq!(format_size(0), "0B");
    assert_eq!(format_size(1_023), "1023B");
    assert_eq!(format_size(1_024), "1.0KB");
    assert_eq!(format_size(1_280), "1.3KB");
    assert_eq!(format_size(1_048_575), "1024.0KB");
    assert_eq!(format_size(1_048_576), "1.0MB");
    assert_eq!(format_size(1_572_864), "1.5MB");
}

#[test]
fn no_truncation_preserves_crlf_unicode_trailing_lf_and_full_metadata() {
    let content = "é\r\n🙂\n";

    assert_eq!(
        truncate_head(content, options(2, 9)),
        result(content, false, None, 2, 9, 2, 9, false, false, 2, 9)
    );
    assert_eq!(
        truncate_tail(content, options(2, 9)),
        result(content, false, None, 2, 9, 2, 9, false, false, 2, 9)
    );
}

#[test]
fn empty_input_with_zero_limits_is_not_truncated() {
    assert_eq!(
        truncate_head("", options(0, 0)),
        result("", false, None, 0, 0, 0, 0, false, false, 0, 0)
    );
    assert_eq!(
        truncate_tail("", options(0, 0)),
        result("", false, None, 0, 0, 0, 0, false, false, 0, 0)
    );
}

#[test]
fn head_byte_limit_keeps_complete_lines() {
    assert_eq!(
        truncate_head("aa\nbbb\nc", options(10, 6)),
        result(
            "aa\nbbb",
            true,
            Some(TruncatedBy::Bytes),
            3,
            8,
            2,
            6,
            false,
            false,
            10,
            6
        )
    );
}

#[test]
fn head_reports_an_oversized_first_line() {
    assert_eq!(
        truncate_head("éé\nabc", options(10, 3)),
        result("", true, Some(TruncatedBy::Bytes), 2, 8, 0, 0, false, true, 10, 3)
    );
}

#[test]
fn head_line_limit_keeps_complete_lines() {
    assert_eq!(
        truncate_head("a\nb\nc", options(2, 100)),
        result("a\nb", true, Some(TruncatedBy::Lines), 3, 5, 2, 3, false, false, 2, 100)
    );
}

#[test]
fn head_reports_lines_when_line_and_byte_limits_are_reached_exactly() {
    assert_eq!(
        truncate_head("a\nb\nc", options(2, 3)),
        result("a\nb", true, Some(TruncatedBy::Lines), 3, 5, 2, 3, false, false, 2, 3)
    );
}

#[test]
fn head_zero_line_limit_returns_empty_output() {
    assert_eq!(
        truncate_head("a\nb", options(0, 100)),
        result("", true, Some(TruncatedBy::Lines), 2, 3, 0, 0, false, false, 0, 100)
    );
}

#[test]
fn tail_line_limit_keeps_complete_suffix_lines() {
    assert_eq!(
        truncate_tail("a\nb\nc", options(2, 100)),
        result("b\nc", true, Some(TruncatedBy::Lines), 3, 5, 2, 3, false, false, 2, 100)
    );
}

#[test]
fn tail_byte_limit_keeps_multiple_complete_suffix_lines() {
    assert_eq!(
        truncate_tail("a\nbb\nccc", options(10, 6)),
        result(
            "bb\nccc",
            true,
            Some(TruncatedBy::Bytes),
            3,
            8,
            2,
            6,
            false,
            false,
            10,
            6
        )
    );
}

#[test]
fn tail_byte_limit_keeps_a_partial_unicode_suffix() {
    assert_eq!(
        truncate_tail("aé🙂b", options(10, 5)),
        result("🙂b", true, Some(TruncatedBy::Bytes), 1, 8, 1, 5, true, false, 10, 5)
    );
}

#[test]
fn tail_drops_an_oversized_scalar_without_splitting_it() {
    assert_eq!(
        truncate_tail("abc🙂", options(10, 3)),
        result("", true, Some(TruncatedBy::Bytes), 1, 7, 1, 0, true, false, 10, 3)
    );
}

#[test]
fn tail_reclassifies_empty_partial_output_as_lines_at_zero_bytes() {
    assert_eq!(
        truncate_tail("abc🙂", options(1, 0)),
        result("", true, Some(TruncatedBy::Lines), 1, 7, 1, 0, true, false, 1, 0)
    );
}

#[test]
fn truncate_line_uses_default_limit_and_exact_marker() {
    let exact = "x".repeat(GREP_MAX_LINE_LENGTH);
    let over = "x".repeat(GREP_MAX_LINE_LENGTH + 1);

    assert_eq!(
        truncate_line(&exact, None),
        TruncatedLine {
            text: exact.clone(),
            was_truncated: false,
        }
    );
    assert_eq!(
        truncate_line(&over, None),
        TruncatedLine {
            text: format!("{exact}{TRUNCATION_MARKER}"),
            was_truncated: true,
        }
    );
}

#[test]
fn truncate_line_supports_zero_and_custom_ascii_limits() {
    assert_eq!(
        truncate_line("abc", Some(0)),
        TruncatedLine {
            text: TRUNCATION_MARKER.to_owned(),
            was_truncated: true,
        }
    );
    assert_eq!(
        truncate_line("abc", Some(2)),
        TruncatedLine {
            text: format!("ab{TRUNCATION_MARKER}"),
            was_truncated: true,
        }
    );
}

#[test]
fn truncate_line_never_splits_a_unicode_scalar() {
    assert_eq!(
        truncate_line("🙂x", Some(1)),
        TruncatedLine {
            text: format!("🙂{TRUNCATION_MARKER}"),
            was_truncated: true,
        }
    );
}

#[test]
fn combining_marks_nul_and_supplementary_scalars_remain_byte_safe() {
    let content = "\u{301}\0🙂";

    assert_eq!(
        truncate_head(content, options(10, 100)),
        result(content, false, None, 1, 7, 1, 7, false, false, 10, 100)
    );
    assert_eq!(
        truncate_tail(content, options(10, 4)),
        result("🙂", true, Some(TruncatedBy::Bytes), 1, 7, 1, 4, true, false, 10, 4)
    );
}
