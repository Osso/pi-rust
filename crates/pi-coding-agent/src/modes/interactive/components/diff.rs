//! Ported from `coding-agent/src/modes/interactive/components/diff.ts`.

use std::sync::OnceLock;

use pi_agent_core::jsdiff::diff_words;
use regex::Regex;

use crate::modes::interactive::theme::{ThemeColor, with_theme};

const JS_WHITESPACE: &str = r"\t-\r \u{00A0}\u{1680}\u{2000}-\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}\u{FEFF}";

fn diff_line_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(&format!(
            "^([+\\-{JS_WHITESPACE}])([{JS_WHITESPACE}]*[0-9]*)[{JS_WHITESPACE}](.*)$"
        ))
        .expect("diff line pattern must compile")
    })
}

struct ParsedDiffLine {
    prefix: char,
    line_num: String,
    content: String,
}

/// Parse a diff line into prefix, line number, and content.
fn parse_diff_line(line: &str) -> Option<ParsedDiffLine> {
    let captures = diff_line_pattern().captures(line)?;
    let prefix = captures.get(1)?.as_str().chars().next()?;
    Some(ParsedDiffLine {
        prefix,
        line_num: captures.get(2)?.as_str().to_string(),
        content: captures.get(3)?.as_str().to_string(),
    })
}

/// Replace tabs with spaces for consistent rendering.
fn replace_tabs(text: &str) -> String {
    text.replace('\t', "   ")
}

/// Compute word-level diff and render with inverse on changed parts.
fn render_intra_line_diff(old_content: &str, new_content: &str) -> (String, String) {
    let word_diff = diff_words(old_content, new_content);
    let mut removed_line = String::new();
    let mut added_line = String::new();
    let mut is_first_removed = true;
    let mut is_first_added = true;

    for part in &word_diff {
        if part.removed {
            let mut value = part.value.as_str();
            if is_first_removed {
                let leading = leading_whitespace(value);
                value = &value[leading.len()..];
                removed_line.push_str(leading);
                is_first_removed = false;
            }
            if !value.is_empty() {
                removed_line.push_str(&with_theme(|theme| theme.inverse(value)));
            }
        } else if part.added {
            let mut value = part.value.as_str();
            if is_first_added {
                let leading = leading_whitespace(value);
                value = &value[leading.len()..];
                added_line.push_str(leading);
                is_first_added = false;
            }
            if !value.is_empty() {
                added_line.push_str(&with_theme(|theme| theme.inverse(value)));
            }
        } else {
            removed_line.push_str(&part.value);
            added_line.push_str(&part.value);
        }
    }

    (removed_line, added_line)
}

fn leading_whitespace(value: &str) -> &str {
    let end = value
        .char_indices()
        .find(|(_, character)| !is_js_whitespace(*character))
        .map_or(value.len(), |(index, _)| index);
    &value[..end]
}

fn is_js_whitespace(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

/// Render a diff string with colored lines and intra-line change highlighting.
pub fn render_diff(diff_text: &str) -> String {
    let lines: Vec<&str> = diff_text.split('\n').collect();
    let mut result: Vec<String> = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let Some(parsed) = parse_diff_line(lines[index]) else {
            result.push(with_theme(|theme| theme.fg(ThemeColor::ToolDiffContext, lines[index])));
            index += 1;
            continue;
        };

        if parsed.prefix == '-' {
            let mut removed_lines: Vec<(String, String)> = Vec::new();
            while index < lines.len() {
                let Some(parsed) = parse_diff_line(lines[index]) else {
                    break;
                };
                if parsed.prefix != '-' {
                    break;
                }
                removed_lines.push((parsed.line_num, parsed.content));
                index += 1;
            }

            let mut added_lines: Vec<(String, String)> = Vec::new();
            while index < lines.len() {
                let Some(parsed) = parse_diff_line(lines[index]) else {
                    break;
                };
                if parsed.prefix != '+' {
                    break;
                }
                added_lines.push((parsed.line_num, parsed.content));
                index += 1;
            }

            if removed_lines.len() == 1 && added_lines.len() == 1 {
                let (removed_line_num, removed_content) = &removed_lines[0];
                let (added_line_num, added_content) = &added_lines[0];
                let (removed_line, added_line) =
                    render_intra_line_diff(&replace_tabs(removed_content), &replace_tabs(added_content));
                result.push(with_theme(|theme| {
                    theme.fg(
                        ThemeColor::ToolDiffRemoved,
                        &format!("-{removed_line_num} {removed_line}"),
                    )
                }));
                result.push(with_theme(|theme| {
                    theme.fg(ThemeColor::ToolDiffAdded, &format!("+{added_line_num} {added_line}"))
                }));
            } else {
                for (line_num, content) in removed_lines {
                    result.push(with_theme(|theme| {
                        theme.fg(
                            ThemeColor::ToolDiffRemoved,
                            &format!("-{line_num} {}", replace_tabs(&content)),
                        )
                    }));
                }
                for (line_num, content) in added_lines {
                    result.push(with_theme(|theme| {
                        theme.fg(
                            ThemeColor::ToolDiffAdded,
                            &format!("+{line_num} {}", replace_tabs(&content)),
                        )
                    }));
                }
            }
        } else if parsed.prefix == '+' {
            result.push(with_theme(|theme| {
                theme.fg(
                    ThemeColor::ToolDiffAdded,
                    &format!("+{} {}", parsed.line_num, replace_tabs(&parsed.content)),
                )
            }));
            index += 1;
        } else {
            result.push(with_theme(|theme| {
                theme.fg(
                    ThemeColor::ToolDiffContext,
                    &format!(" {} {}", parsed.line_num, replace_tabs(&parsed.content)),
                )
            }));
            index += 1;
        }
    }

    result.join("\n")
}
