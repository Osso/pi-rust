//! Ported from `coding-agent/src/modes/interactive/components/diff.ts`.

use crate::core::tools::jsdiff::diff_words;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::string::is_js_whitespace_unit;

use crate::modes::interactive::theme::{ThemeColor, with_theme};

struct ParsedDiffLine {
    prefix: u16,
    line_num: JsString,
    content: JsString,
}

/// Match the source's prefix/whitespace/digits regex without decoding content.
fn parse_diff_line(line: &JsString) -> Option<ParsedDiffLine> {
    let units = line.units();
    let prefix = *units.first()?;
    if prefix != u16::from(b'+') && prefix != u16::from(b'-') && !is_js_whitespace_unit(prefix) {
        return None;
    }
    let separator = find_diff_separator(units)?;
    let content = line.slice(separator + 1, line.len());
    if content
        .units()
        .iter()
        .any(|unit| matches!(unit, 0x000a | 0x000d | 0x2028 | 0x2029))
    {
        return None;
    }
    Some(ParsedDiffLine {
        prefix,
        line_num: line.slice(1, separator),
        content,
    })
}

fn find_diff_separator(units: &[u16]) -> Option<usize> {
    let whitespace_end = 1 + units[1..]
        .iter()
        .take_while(|unit| is_js_whitespace_unit(**unit))
        .count();
    let digits_end = whitespace_end
        + units[whitespace_end..]
            .iter()
            .take_while(|unit| (u16::from(b'0')..=u16::from(b'9')).contains(unit))
            .count();
    let separator = if units.get(digits_end).is_some_and(|unit| is_js_whitespace_unit(*unit)) {
        digits_end
    } else if digits_end == whitespace_end && whitespace_end > 1 {
        whitespace_end - 1
    } else {
        return None;
    };
    Some(separator)
}

fn replace_tabs(text: &JsString) -> JsString {
    text.replace("\t", "   ")
}

/// Compute word-level diff and render with inverse on changed parts.
fn render_intra_line_diff(old_content: &JsString, new_content: &JsString) -> (JsString, JsString) {
    let word_diff = diff_words(old_content, new_content);
    let mut removed_line = JsString::default();
    let mut added_line = JsString::default();
    let mut is_first_removed = true;
    let mut is_first_added = true;

    for part in &word_diff {
        if part.removed {
            let mut value = part.value.clone();
            if is_first_removed {
                let leading = leading_whitespace(&value);
                value = value.slice(leading.len(), value.len());
                removed_line.push_str(&leading);
                is_first_removed = false;
            }
            if !value.is_empty() {
                removed_line.push_str(with_theme(|theme| theme.inverse(&value)));
            }
        } else if part.added {
            let mut value = part.value.clone();
            if is_first_added {
                let leading = leading_whitespace(&value);
                value = value.slice(leading.len(), value.len());
                added_line.push_str(&leading);
                is_first_added = false;
            }
            if !value.is_empty() {
                added_line.push_str(with_theme(|theme| theme.inverse(&value)));
            }
        } else {
            removed_line.push_str(&part.value);
            added_line.push_str(&part.value);
        }
    }

    (removed_line, added_line)
}

fn leading_whitespace(value: &JsString) -> JsString {
    let end = value
        .units()
        .iter()
        .take_while(|unit| is_js_whitespace_unit(**unit))
        .count();
    value.slice(0, end)
}

/// Render a diff string with colored lines and intra-line change highlighting.
pub fn render_diff(diff_text: &JsString) -> JsString {
    let lines: Vec<JsString> = diff_text.split('\n').collect();
    let mut result: Vec<JsString> = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let Some(parsed) = parse_diff_line(&lines[index]) else {
            result.push(with_theme(|theme| theme.fg(ThemeColor::ToolDiffContext, &lines[index])));
            index += 1;
            continue;
        };

        if parsed.prefix == u16::from(b'-') {
            let mut removed_lines: Vec<(JsString, JsString)> = Vec::new();
            while index < lines.len() {
                let Some(parsed) = parse_diff_line(&lines[index]) else {
                    break;
                };
                if parsed.prefix != u16::from(b'-') {
                    break;
                }
                removed_lines.push((parsed.line_num, parsed.content));
                index += 1;
            }

            let mut added_lines: Vec<(JsString, JsString)> = Vec::new();
            while index < lines.len() {
                let Some(parsed) = parse_diff_line(&lines[index]) else {
                    break;
                };
                if parsed.prefix != u16::from(b'+') {
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
                        js_format!("-{} {}", removed_line_num, removed_line),
                    )
                }));
                result.push(with_theme(|theme| {
                    theme.fg(
                        ThemeColor::ToolDiffAdded,
                        js_format!("+{} {}", added_line_num, added_line),
                    )
                }));
            } else {
                for (line_num, content) in removed_lines {
                    result.push(with_theme(|theme| {
                        theme.fg(
                            ThemeColor::ToolDiffRemoved,
                            js_format!("-{} {}", line_num, replace_tabs(&content)),
                        )
                    }));
                }
                for (line_num, content) in added_lines {
                    result.push(with_theme(|theme| {
                        theme.fg(
                            ThemeColor::ToolDiffAdded,
                            js_format!("+{} {}", line_num, replace_tabs(&content)),
                        )
                    }));
                }
            }
        } else if parsed.prefix == u16::from(b'+') {
            result.push(with_theme(|theme| {
                theme.fg(
                    ThemeColor::ToolDiffAdded,
                    js_format!("+{} {}", parsed.line_num, replace_tabs(&parsed.content)),
                )
            }));
            index += 1;
        } else {
            result.push(with_theme(|theme| {
                theme.fg(
                    ThemeColor::ToolDiffContext,
                    js_format!(" {} {}", parsed.line_num, replace_tabs(&parsed.content)),
                )
            }));
            index += 1;
        }
    }

    JsString::join(&result, "\n")
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    use pi_js_compat::js_json::JsString;

    #[test]
    fn distinct_surrogates_are_highlighted_before_terminal_encoding() {
        crate::modes::interactive::theme::init_theme(Some(&JsString::from("dark")), false).unwrap();
        let old = JsString::from_utf16(vec![0x61, 0xd800, 0x62]);
        let new = JsString::from_utf16(vec![0x61, 0xd801, 0x62]);
        let (removed, added) = render_intra_line_diff(&old, &new);
        assert_eq!(
            JsString::from(removed.clone()),
            JsString::from("a") + with_theme(|theme| theme.inverse(JsString::from_utf16(vec![0xd800]))) + "b"
        );
        assert_eq!(
            JsString::from(added.clone()),
            JsString::from("a") + with_theme(|theme| theme.inverse(JsString::from_utf16(vec![0xd801]))) + "b"
        );
        assert_ne!(added, removed);
    }

    #[test]
    fn paired_surrogate_and_literal_replacement_remain_distinct() {
        crate::modes::interactive::theme::init_theme(Some(&JsString::from("dark")), false).unwrap();
        let input = JsString::from_utf16(vec![45, 49, 32, 0xd83d, 0xde00, 10, 43, 49, 32, 0xfffd]);
        let output = render_diff(&input);
        assert!(output.contains("😀"));
        assert!(output.contains(&with_theme(|theme| theme.inverse("�"))));
    }
}
