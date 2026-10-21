//! Lossless port of coding-agent `tools/truncate.ts`; byte suffix decoding matches Node Buffer.

use serde::{Deserialize, Serialize};

pub use pi_agent_core::harness::utils::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH, TruncatedBy, TruncationOptions, format_size,
};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
const TRUNCATION_MARKER: &str = "... [truncated]";

/// Truncated output and source-compatible metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub max_lines: usize,
    pub max_bytes: usize,
}

/// Single-line truncation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncatedLine {
    pub text: JsString,
    pub was_truncated: bool,
}

/// Keeps complete lines from the start of the content.
pub fn truncate_head(content: impl Into<JsString>, options: TruncationOptions) -> TruncationResult {
    let content = content.into();
    let content = &content;
    let lines = split_lines_for_counting(content);
    let totals = Totals::new(content, &lines);
    if totals.fit(options) {
        return untruncated_result(content, totals, options);
    }

    let first_line_bytes = lines.first().map_or(0, |line| encode_node_utf8(line).len());
    if first_line_bytes > options.max_bytes {
        return empty_head_byte_result(totals, options);
    }

    let mut output = Vec::new();
    let mut output_bytes = 0;
    let mut truncated_by = TruncatedBy::Lines;
    for (index, line) in lines.iter().take(options.max_lines).enumerate() {
        let line_bytes = encode_node_utf8(line).len() + usize::from(index > 0);
        if output_bytes + line_bytes > options.max_bytes {
            truncated_by = TruncatedBy::Bytes;
            break;
        }
        output.push(line.clone());
        output_bytes += line_bytes;
    }

    if output.len() >= options.max_lines && output_bytes <= options.max_bytes {
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
pub fn truncate_line(line: impl Into<JsString>, max_chars: Option<usize>) -> TruncatedLine {
    let line = line.into();
    let max_chars = max_chars.unwrap_or(GREP_MAX_LINE_LENGTH);
    if line.len() <= max_chars {
        return TruncatedLine {
            text: line,
            was_truncated: false,
        };
    }
    TruncatedLine {
        text: JsString::from_utf16(line.units()[..max_chars].to_vec()) + TRUNCATION_MARKER,
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
        self.lines <= options.max_lines && self.bytes <= options.max_bytes
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
    for line in lines.iter().rev().take(options.max_lines) {
        let line_bytes = encode_node_utf8(line).len() + usize::from(!output.is_empty());
        if output_bytes + line_bytes > options.max_bytes {
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

fn split_lines_for_counting(content: &JsString) -> Vec<JsString> {
    if content.is_empty() {
        return Vec::new();
    }

    let mut lines = content.split('\n').collect::<Vec<_>>();
    if content.ends_with('\n') {
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

fn truncate_string_to_bytes_from_end(value: &JsString, max_bytes: usize) -> JsString {
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
