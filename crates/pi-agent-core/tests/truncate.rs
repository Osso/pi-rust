use pi_agent_core::harness::utils::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH, TruncatedBy, TruncatedLine, TruncationOptions,
    TruncationResult, format_size, truncate_head, truncate_line, truncate_tail,
};

const TRUNCATION_MARKER: &str = "... [truncated]";

fn options(max_lines: usize, max_bytes: usize) -> TruncationOptions {
    TruncationOptions {
        max_lines: max_lines as f64,
        max_bytes: max_bytes as f64,
    }
}

#[derive(Clone, Copy)]
struct Counts {
    total_lines: usize,
    total_bytes: usize,
    output_lines: usize,
    output_bytes: usize,
}

const fn counts(total_lines: usize, total_bytes: usize, output_lines: usize, output_bytes: usize) -> Counts {
    Counts {
        total_lines,
        total_bytes,
        output_lines,
        output_bytes,
    }
}

#[derive(Clone, Copy)]
struct Flags {
    last_line_partial: bool,
    first_line_exceeds_limit: bool,
}

const NO_FLAGS: Flags = Flags {
    last_line_partial: false,
    first_line_exceeds_limit: false,
};
const LAST_LINE_PARTIAL: Flags = Flags {
    last_line_partial: true,
    first_line_exceeds_limit: false,
};
const FIRST_LINE_EXCEEDS_LIMIT: Flags = Flags {
    last_line_partial: false,
    first_line_exceeds_limit: true,
};

fn result(
    content: &str,
    truncated_by: Option<TruncatedBy>,
    counts: Counts,
    flags: Flags,
    limits: TruncationOptions,
) -> TruncationResult {
    TruncationResult {
        content: content.into(),
        truncated: truncated_by.is_some(),
        truncated_by,
        total_lines: counts.total_lines,
        total_bytes: counts.total_bytes,
        output_lines: counts.output_lines,
        output_bytes: counts.output_bytes,
        last_line_partial: flags.last_line_partial,
        first_line_exceeds_limit: flags.first_line_exceeds_limit,
        max_lines: limits.max_lines,
        max_bytes: limits.max_bytes,
    }
}

#[test]
fn constants_and_default_options_match_source() {
    assert_eq!(DEFAULT_MAX_LINES, 2_000);
    assert_eq!(DEFAULT_MAX_BYTES, 10 * 1024);
    assert_eq!(GREP_MAX_LINE_LENGTH, 500);

    let defaults = TruncationOptions::default();
    assert_eq!(defaults.max_lines, DEFAULT_MAX_LINES as f64);
    assert_eq!(defaults.max_bytes, DEFAULT_MAX_BYTES as f64);
}

#[test]
fn format_size_matches_byte_kilobyte_and_megabyte_thresholds() {
    assert_eq!(format_size(0.0), "0B");
    assert_eq!(format_size(1_023.0), "1023B");
    assert_eq!(format_size(1_024.0), "1.0KB");
    assert_eq!(format_size(1_280.0), "1.3KB");
    assert_eq!(format_size(1_048_575.0), "1024.0KB");
    assert_eq!(format_size(1_048_576.0), "1.0MB");
    assert_eq!(format_size(1_572_864.0), "1.5MB");
}

#[test]
fn head_counts_terminal_empty_line_while_tail_does_not() {
    let content = "é\r\n🙂\n";

    assert_eq!(
        truncate_head(content, options(2, 9)),
        result(
            "é\r\n🙂",
            Some(TruncatedBy::Lines),
            counts(3, 9, 2, 8),
            NO_FLAGS,
            options(2, 9)
        )
    );
    assert_eq!(
        truncate_tail(content, options(2, 9)),
        result(content, None, counts(2, 9, 2, 9), NO_FLAGS, options(2, 9))
    );
}

#[test]
fn empty_input_still_has_one_split_line_and_zero_limits_truncate_it() {
    assert_eq!(
        truncate_head("", options(0, 0)),
        result(
            "",
            Some(TruncatedBy::Lines),
            counts(1, 0, 0, 0),
            NO_FLAGS,
            options(0, 0)
        )
    );
    assert_eq!(
        truncate_tail("", options(0, 0)),
        result(
            "",
            Some(TruncatedBy::Lines),
            counts(1, 0, 0, 0),
            NO_FLAGS,
            options(0, 0)
        )
    );
}

#[test]
fn head_byte_limit_keeps_complete_lines() {
    assert_eq!(
        truncate_head("aa\nbbb\nc", options(10, 6)),
        result(
            "aa\nbbb",
            Some(TruncatedBy::Bytes),
            counts(3, 8, 2, 6),
            NO_FLAGS,
            options(10, 6)
        )
    );
}

#[test]
fn head_reports_an_oversized_first_line() {
    assert_eq!(
        truncate_head("éé\nabc", options(10, 3)),
        result(
            "",
            Some(TruncatedBy::Bytes),
            counts(2, 8, 0, 0),
            FIRST_LINE_EXCEEDS_LIMIT,
            options(10, 3)
        )
    );
}

#[test]
fn head_line_limit_keeps_complete_lines() {
    assert_eq!(
        truncate_head("a\nb\nc", options(2, 100)),
        result(
            "a\nb",
            Some(TruncatedBy::Lines),
            counts(3, 5, 2, 3),
            NO_FLAGS,
            options(2, 100)
        )
    );
}

#[test]
fn head_reports_lines_when_line_and_byte_limits_are_reached_exactly() {
    assert_eq!(
        truncate_head("a\nb\nc", options(2, 3)),
        result(
            "a\nb",
            Some(TruncatedBy::Lines),
            counts(3, 5, 2, 3),
            NO_FLAGS,
            options(2, 3)
        )
    );
}

#[test]
fn head_zero_line_limit_returns_empty_output() {
    assert_eq!(
        truncate_head("a\nb", options(0, 100)),
        result(
            "",
            Some(TruncatedBy::Lines),
            counts(2, 3, 0, 0),
            NO_FLAGS,
            options(0, 100)
        )
    );
}

#[test]
fn tail_line_limit_keeps_complete_suffix_lines() {
    assert_eq!(
        truncate_tail("a\nb\nc", options(2, 100)),
        result(
            "b\nc",
            Some(TruncatedBy::Lines),
            counts(3, 5, 2, 3),
            NO_FLAGS,
            options(2, 100)
        )
    );
}

#[test]
fn tail_byte_limit_keeps_multiple_complete_suffix_lines() {
    assert_eq!(
        truncate_tail("a\nbb\nccc", options(10, 6)),
        result(
            "bb\nccc",
            Some(TruncatedBy::Bytes),
            counts(3, 8, 2, 6),
            NO_FLAGS,
            options(10, 6)
        )
    );
}

#[test]
fn tail_byte_limit_keeps_a_partial_unicode_suffix() {
    assert_eq!(
        truncate_tail("aé🙂b", options(10, 5)),
        result(
            "🙂b",
            Some(TruncatedBy::Bytes),
            counts(1, 8, 1, 5),
            LAST_LINE_PARTIAL,
            options(10, 5)
        )
    );
}

#[test]
fn tail_drops_an_oversized_scalar_without_splitting_it() {
    assert_eq!(
        truncate_tail("abc🙂", options(10, 3)),
        result(
            "",
            Some(TruncatedBy::Bytes),
            counts(1, 7, 1, 0),
            LAST_LINE_PARTIAL,
            options(10, 3)
        )
    );
}

#[test]
fn tail_reclassifies_empty_partial_output_as_lines_at_zero_bytes() {
    assert_eq!(
        truncate_tail("abc🙂", options(1, 0)),
        result(
            "",
            Some(TruncatedBy::Lines),
            counts(1, 7, 1, 0),
            LAST_LINE_PARTIAL,
            options(1, 0)
        )
    );
}

#[test]
fn truncate_line_uses_default_limit_and_exact_marker() {
    let exact = "x".repeat(GREP_MAX_LINE_LENGTH);
    let over = "x".repeat(GREP_MAX_LINE_LENGTH + 1);

    assert_eq!(
        truncate_line(&exact, None),
        TruncatedLine {
            text: exact.clone().into(),
            was_truncated: false,
        }
    );
    assert_eq!(
        truncate_line(&over, None),
        TruncatedLine {
            text: format!("{exact}{TRUNCATION_MARKER}").into(),
            was_truncated: true,
        }
    );
}

#[test]
fn truncate_line_supports_zero_and_custom_ascii_limits() {
    assert_eq!(
        truncate_line("abc", Some(0.0)),
        TruncatedLine {
            text: TRUNCATION_MARKER.into(),
            was_truncated: true,
        }
    );
    assert_eq!(
        truncate_line("abc", Some(2.0)),
        TruncatedLine {
            text: format!("ab{TRUNCATION_MARKER}").into(),
            was_truncated: true,
        }
    );
}

#[test]
fn truncate_line_slices_utf16_units_even_inside_a_scalar() {
    assert_eq!(
        truncate_line("🙂x", Some(1.0)),
        TruncatedLine {
            text: pi_js_compat::js_json::JsString::from_utf16(vec![0xd83d]) + TRUNCATION_MARKER,
            was_truncated: true,
        }
    );
}

#[test]
fn combining_marks_nul_and_supplementary_scalars_remain_byte_safe() {
    let content = "\u{301}\0🙂";

    assert_eq!(
        truncate_head(content, options(10, 100)),
        result(content, None, counts(1, 7, 1, 7), NO_FLAGS, options(10, 100))
    );
    assert_eq!(
        truncate_tail(content, options(10, 4)),
        result(
            "🙂",
            Some(TruncatedBy::Bytes),
            counts(1, 7, 1, 4),
            LAST_LINE_PARTIAL,
            options(10, 4)
        )
    );
}
