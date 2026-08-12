/// Default maximum number of output lines.
pub const DEFAULT_MAX_LINES: usize = 2_000;
/// Default maximum number of output bytes.
pub const DEFAULT_MAX_BYTES: usize = 10 * 1024;
/// Default maximum number of Unicode scalar values in one grep match line.
pub const GREP_MAX_LINE_LENGTH: usize = 500;

const TRUNCATION_MARKER: &str = "... [truncated]";
const KIBIBYTE: usize = 1024;
const MEBIBYTE: usize = KIBIBYTE * KIBIBYTE;

/// Line and byte limits applied by head and tail truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TruncationOptions {
    pub max_lines: usize,
    pub max_bytes: usize,
}

impl Default for TruncationOptions {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_MAX_LINES,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

/// Identifies the limit that truncated output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncatedBy {
    Lines,
    Bytes,
}

/// Truncated output and source-compatible metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationResult {
    pub content: String,
    pub truncated: bool,
    pub truncated_by: Option<TruncatedBy>,
    pub total_lines: usize,
    pub total_bytes: usize,
    pub output_lines: usize,
    pub output_bytes: usize,
    pub last_line_partial: bool,
    pub first_line_exceeds_limit: bool,
    pub max_lines: usize,
    pub max_bytes: usize,
}

/// Single-line truncation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncatedLine {
    pub text: String,
    pub was_truncated: bool,
}

/// Formats a non-negative byte count with the source B, KB, or MB suffix.
pub fn format_size(bytes: usize) -> String {
    if bytes < KIBIBYTE {
        return format!("{bytes}B");
    }
    if bytes < MEBIBYTE {
        return format_decimal_size(bytes, KIBIBYTE, "KB");
    }
    format_decimal_size(bytes, MEBIBYTE, "MB")
}

/// Keeps complete lines from the start of the content.
pub fn truncate_head(content: &str, options: TruncationOptions) -> TruncationResult {
    let lines = split_lines_for_counting(content);
    let totals = Totals::new(content, &lines);
    if totals.fit(options) {
        return untruncated_result(content, totals, options);
    }

    let first_line_bytes = lines.first().map_or(0, |line| line.len());
    if first_line_bytes > options.max_bytes {
        return empty_head_byte_result(totals, options);
    }

    let mut output = Vec::new();
    let mut output_bytes = 0;
    let mut truncated_by = TruncatedBy::Lines;
    for (index, line) in lines.iter().take(options.max_lines).enumerate() {
        let line_bytes = line.len() + usize::from(index > 0);
        if output_bytes + line_bytes > options.max_bytes {
            truncated_by = TruncatedBy::Bytes;
            break;
        }
        output.push(*line);
        output_bytes += line_bytes;
    }

    if output.len() >= options.max_lines && output_bytes <= options.max_bytes {
        truncated_by = TruncatedBy::Lines;
    }
    let output_lines = output.len();
    truncated_result(
        output.join("\n"),
        output_lines,
        truncated_by,
        totals,
        options,
        false,
        false,
    )
}

/// Keeps complete lines from the end of the content, or a byte-safe suffix of the final line.
pub fn truncate_tail(content: &str, options: TruncationOptions) -> TruncationResult {
    let lines = split_lines_for_counting(content);
    let totals = Totals::new(content, &lines);
    if totals.fit(options) {
        return untruncated_result(content, totals, options);
    }

    let selection = collect_tail_lines(&lines, options);
    truncated_result(
        selection.lines.join("\n"),
        selection.lines.len(),
        selection.truncated_by,
        totals,
        options,
        selection.last_line_partial,
        false,
    )
}

/// Truncates one line by Unicode scalar count and appends the exact source marker.
pub fn truncate_line(line: &str, max_chars: Option<usize>) -> TruncatedLine {
    let max_chars = max_chars.unwrap_or(GREP_MAX_LINE_LENGTH);
    if line.chars().count() <= max_chars {
        return TruncatedLine {
            text: line.to_owned(),
            was_truncated: false,
        };
    }

    let mut text = line.chars().take(max_chars).collect::<String>();
    text.push_str(TRUNCATION_MARKER);
    TruncatedLine {
        text,
        was_truncated: true,
    }
}

#[derive(Debug, Clone, Copy)]
struct Totals {
    lines: usize,
    bytes: usize,
}

impl Totals {
    fn new(content: &str, lines: &[&str]) -> Self {
        Self {
            lines: lines.len(),
            bytes: content.len(),
        }
    }

    fn fit(self, options: TruncationOptions) -> bool {
        self.lines <= options.max_lines && self.bytes <= options.max_bytes
    }
}

struct TailSelection {
    lines: Vec<String>,
    truncated_by: TruncatedBy,
    last_line_partial: bool,
}

fn collect_tail_lines(lines: &[&str], options: TruncationOptions) -> TailSelection {
    let mut output = Vec::new();
    let mut output_bytes = 0;
    let mut truncated_by = TruncatedBy::Lines;
    let mut last_line_partial = false;
    for line in lines.iter().rev().take(options.max_lines) {
        let line_bytes = line.len() + usize::from(!output.is_empty());
        if output_bytes + line_bytes > options.max_bytes {
            truncated_by = TruncatedBy::Bytes;
            if output.is_empty() {
                let partial = truncate_string_to_bytes_from_end(line, options.max_bytes);
                output_bytes = partial.len();
                output.push(partial);
                last_line_partial = true;
            }
            break;
        }
        output.push((*line).to_owned());
        output_bytes += line_bytes;
    }

    if output.len() >= options.max_lines && output_bytes <= options.max_bytes {
        truncated_by = TruncatedBy::Lines;
    }
    output.reverse();
    TailSelection {
        lines: output,
        truncated_by,
        last_line_partial,
    }
}

fn format_decimal_size(bytes: usize, divisor: usize, suffix: &str) -> String {
    let rounded_tenths = ((bytes as u128) * 10 + (divisor as u128 / 2)) / divisor as u128;
    format!("{}.{}{suffix}", rounded_tenths / 10, rounded_tenths % 10)
}

fn split_lines_for_counting(content: &str) -> Vec<&str> {
    if content.is_empty() {
        return Vec::new();
    }

    let mut lines = content.split('\n').collect::<Vec<_>>();
    if content.ends_with('\n') {
        lines.pop();
    }
    lines
}

fn untruncated_result(content: &str, totals: Totals, options: TruncationOptions) -> TruncationResult {
    TruncationResult {
        content: content.to_owned(),
        truncated: false,
        truncated_by: None,
        total_lines: totals.lines,
        total_bytes: totals.bytes,
        output_lines: totals.lines,
        output_bytes: totals.bytes,
        last_line_partial: false,
        first_line_exceeds_limit: false,
        max_lines: options.max_lines,
        max_bytes: options.max_bytes,
    }
}

fn empty_head_byte_result(totals: Totals, options: TruncationOptions) -> TruncationResult {
    TruncationResult {
        content: String::new(),
        truncated: true,
        truncated_by: Some(TruncatedBy::Bytes),
        total_lines: totals.lines,
        total_bytes: totals.bytes,
        output_lines: 0,
        output_bytes: 0,
        last_line_partial: false,
        first_line_exceeds_limit: true,
        max_lines: options.max_lines,
        max_bytes: options.max_bytes,
    }
}

fn truncated_result(
    content: String,
    output_lines: usize,
    truncated_by: TruncatedBy,
    totals: Totals,
    options: TruncationOptions,
    last_line_partial: bool,
    first_line_exceeds_limit: bool,
) -> TruncationResult {
    let output_bytes = content.len();
    TruncationResult {
        content,
        truncated: true,
        truncated_by: Some(truncated_by),
        total_lines: totals.lines,
        total_bytes: totals.bytes,
        output_lines,
        output_bytes,
        last_line_partial,
        first_line_exceeds_limit,
        max_lines: options.max_lines,
        max_bytes: options.max_bytes,
    }
}

fn truncate_string_to_bytes_from_end(value: &str, max_bytes: usize) -> String {
    let mut start = value.len();
    let mut output_bytes = 0;
    for (index, character) in value.char_indices().rev() {
        let character_bytes = character.len_utf8();
        if output_bytes + character_bytes > max_bytes {
            break;
        }
        output_bytes += character_bytes;
        start = index;
    }
    value[start..].to_owned()
}
