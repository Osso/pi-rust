//! Frozen agent-core truncation: UTF-16 text, Node UTF-8 byte accounting.

use serde::{Deserialize, Serialize};

/// Default maximum number of output lines.
pub const DEFAULT_MAX_LINES: usize = 2_000;
/// Default maximum number of output bytes.
pub const DEFAULT_MAX_BYTES: usize = 10 * 1024;
/// Default maximum number of Unicode scalar values in one grep match line.
pub const GREP_MAX_LINE_LENGTH: usize = 500;

const KIBIBYTE: usize = 1024;
const MEBIBYTE: usize = KIBIBYTE * KIBIBYTE;

/// Line and byte limits applied by head and tail truncation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TruncationOptions {
    pub max_lines: f64,
    pub max_bytes: f64,
}

impl Default for TruncationOptions {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_MAX_LINES as f64,
            max_bytes: DEFAULT_MAX_BYTES as f64,
        }
    }
}

/// Identifies the limit that truncated output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TruncatedBy {
    Lines,
    Bytes,
}

use pi_js_compat::js_json::{JsString, encode_node_utf8};
const TRUNCATION_MARKER: &str = "... [truncated]";

/// Truncated output and source-compatible metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TruncationResult {
    pub content: JsString,
    pub truncated: bool,
    pub truncated_by: Option<TruncatedBy>,
    pub total_lines: usize,
    pub total_bytes: usize,
    pub output_lines: usize,
    pub output_bytes: usize,
    pub last_line_partial: bool,
    pub first_line_exceeds_limit: bool,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub max_lines: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub max_bytes: f64,
}

/// Single-line truncation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncatedLine {
    pub text: JsString,
    pub was_truncated: bool,
}

/// Formats the full frozen JavaScript number domain.
pub fn format_size(bytes: f64) -> String {
    use pi_js_compat::number::{js_number_to_string, to_fixed};
    if bytes < KIBIBYTE as f64 {
        return format!("{}B", js_number_to_string(bytes));
    }
    if bytes < MEBIBYTE as f64 {
        return format!("{}KB", to_fixed(bytes / KIBIBYTE as f64, 1));
    }
    format!("{}MB", to_fixed(bytes / MEBIBYTE as f64, 1))
}

/// Keeps complete lines from the start of the content.
pub fn truncate_head(content: impl Into<JsString>, options: TruncationOptions) -> TruncationResult {
    let content = content.into();
    let content = &content;
    let lines = content.split('\n').collect::<Vec<_>>();
    let totals = Totals::new(content, &lines);
    if totals.fit(options) {
        return untruncated_result(content, totals, options);
    }

    let first_line_bytes = lines.first().map_or(0, |line| encode_node_utf8(line).len());
    if first_line_bytes as f64 > options.max_bytes {
        return empty_head_byte_result(totals, options);
    }

    let mut output = Vec::new();
    let mut output_bytes = 0;
    let mut truncated_by = TruncatedBy::Lines;
    for (index, line) in lines.iter().enumerate() {
        let within_limit = (index as f64) < options.max_lines;
        if !within_limit {
            break;
        }
        let line_bytes = encode_node_utf8(line).len() + usize::from(index > 0);
        if (output_bytes + line_bytes) as f64 > options.max_bytes {
            truncated_by = TruncatedBy::Bytes;
            break;
        }
        output.push(line.clone());
        output_bytes += line_bytes;
    }

    if output.len() as f64 >= options.max_lines && output_bytes as f64 <= options.max_bytes {
        truncated_by = TruncatedBy::Lines;
    }
    let output_lines = output.len();
    truncated_result(
        JsString::join(output, "\n"),
        output_lines,
        truncated_by,
        totals,
        options,
        false,
        false,
    )
}

/// Keeps complete lines from the end of the content, or a byte-safe suffix of the final line.
pub fn truncate_tail(content: impl Into<JsString>, options: TruncationOptions) -> TruncationResult {
    let content = content.into();
    let content = &content;
    let lines = split_lines_for_counting(content);
    let totals = Totals::new(content, &lines);
    if totals.fit(options) {
        return untruncated_result(content, totals, options);
    }

    let selection = collect_tail_lines(&lines, options);
    truncated_result(
        JsString::join(&selection.lines, "\n"),
        selection.lines.len(),
        selection.truncated_by,
        totals,
        options,
        selection.last_line_partial,
        false,
    )
}

/// Truncates one line by Unicode scalar count and appends the exact source marker.
pub fn truncate_line(line: impl Into<JsString>, max_chars: Option<f64>) -> TruncatedLine {
    let line = line.into();
    let max_chars = max_chars.unwrap_or(GREP_MAX_LINE_LENGTH as f64);
    if line.len() as f64 <= max_chars {
        return TruncatedLine {
            text: line,
            was_truncated: false,
        };
    }
    TruncatedLine {
        text: line.slice(0, slice_end(line.len(), max_chars)) + TRUNCATION_MARKER,
        was_truncated: true,
    }
}

#[derive(Debug, Clone, Copy)]
struct Totals {
    lines: usize,
    bytes: usize,
}

impl Totals {
    fn new(content: &JsString, lines: &[JsString]) -> Self {
        Self {
            lines: lines.len(),
            bytes: encode_node_utf8(content).len(),
        }
    }

    fn fit(self, options: TruncationOptions) -> bool {
        self.lines as f64 <= options.max_lines && self.bytes as f64 <= options.max_bytes
    }
}

struct TailSelection {
    lines: Vec<JsString>,
    truncated_by: TruncatedBy,
    last_line_partial: bool,
}

fn collect_tail_lines(lines: &[JsString], options: TruncationOptions) -> TailSelection {
    let mut output = Vec::new();
    let mut output_bytes = 0;
    let mut truncated_by = TruncatedBy::Lines;
    let mut last_line_partial = false;
    for line in lines.iter().rev() {
        let within_limit = (output.len() as f64) < options.max_lines;
        if !within_limit {
            break;
        }
        let line_bytes = encode_node_utf8(line).len() + usize::from(!output.is_empty());
        if (output_bytes + line_bytes) as f64 > options.max_bytes {
            truncated_by = TruncatedBy::Bytes;
            if output.is_empty() {
                let partial = truncate_string_to_bytes_from_end(line, options.max_bytes);
                output_bytes = encode_node_utf8(&partial).len();
                output.push(partial);
                last_line_partial = true;
            }
            break;
        }
        output.push((*line).to_owned());
        output_bytes += line_bytes;
    }

    if output.len() as f64 >= options.max_lines && output_bytes as f64 <= options.max_bytes {
        truncated_by = TruncatedBy::Lines;
    }
    output.reverse();
    TailSelection {
        lines: output,
        truncated_by,
        last_line_partial,
    }
}

fn split_lines_for_counting(content: &JsString) -> Vec<JsString> {
    let mut lines = content.split('\n').collect::<Vec<_>>();
    if lines.len() > 1 && lines.last().is_some_and(JsString::is_empty) {
        lines.pop();
    }
    lines
}

fn untruncated_result(content: &JsString, totals: Totals, options: TruncationOptions) -> TruncationResult {
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
        content: JsString::default(),
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
    content: JsString,
    output_lines: usize,
    truncated_by: TruncatedBy,
    totals: Totals,
    options: TruncationOptions,
    last_line_partial: bool,
    first_line_exceeds_limit: bool,
) -> TruncationResult {
    let output_bytes = encode_node_utf8(&content).len();
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

fn slice_end(length: usize, end: f64) -> usize {
    let integer = end.trunc();
    if integer < 0.0 {
        length.saturating_sub((-integer) as usize)
    } else {
        (integer as usize).min(length)
    }
}

fn truncate_string_to_bytes_from_end(value: &JsString, max_bytes: f64) -> JsString {
    if max_bytes <= 0.0 {
        return JsString::default();
    }
    let max_bytes = max_bytes as usize;
    let encoded = encode_node_utf8(value);
    if encoded.len() <= max_bytes {
        return value.clone();
    }
    let mut start = encoded.len() - max_bytes;
    while start < encoded.len() && !encoded.is_char_boundary(start) {
        start += 1;
    }
    encoded[start..].into()
}
